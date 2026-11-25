//==============================================================================
// AVIF 그림 풀기. (데스크탑, core/image.rs 가 부른다)
// - 원본은 브라우저가 AVIF 를 푼다. (크롬은 libavif 를 러스트로 옮긴 CrabbyAvif 와 dav1d) 웹 판도 브라우저가 푼다. 데스크탑은 순수 러스트로 푼다.
//   HEIF 상자는 여기서 읽고(libavif src/read.c 와 같은 규칙), AV1 은 dav1d 를 러스트로 옮긴 rav1d 가 YUV 로 푼다.
// - 상자: meta 의 pitm(주 그림), iinf(항목 형 av01, grid), iloc(자료 자리, 파일 안과 idat 안), iref(auxl, dimg, prem), iprp 의 ipco 와 ipma(auxC, colr, irot, imir, clap).
//   - 색 정보: libavif 처럼 주 그림에 colr(nclx)이 있으면 그 matrix_coefficients 와 full_range_flag 를, 없으면 AV1 순서 머리의 것을 쓴다.
//   - grid: 타일(dimg 차례)을 YUV 로 풀어 줄과 칸으로 이어 붙이고 출력 크기로 자른 뒤 한 번에 RGB 로 바꾼다. (libavif 처럼)
//   - 알파: 주 그림을 auxl 로 가리키고 auxC 가 urn:mpeg:mpegB:cicp:systems:auxiliary:alpha 나 urn:mpeg:hevc:2015:auxid:1 인 항목. (libavif isAlphaURN 의 둘)
//     주 그림이 prem 으로 알파를 가리키면 색이 이미 알파로 곱해진 것이라 그대로 프리멀티플라이드 픽셀로 쓴다.
//   - 크롬 AVIFImageDecoder 처럼 clap(깨끗한 테두리)은 자른 자리의 원점이 (0, 0) 일 때만 오른쪽과 아래를 자르고, 아니면 그림 전체를 쓴다.
//     그다음 irot(반시계 angle x 90 도), imir(axis 0 은 위아래, 1 은 좌우를 바꾼다) 차례로 건다. (MIAF 7.3.6.7 의 차례)
// - YUV 를 RGB 로 바꾸는 셈은 libavif(src/reformat.c, src/colr.c)의 기준 셈을 따른다.
//   - 색 계수(Kr, Kb)는 matrix_coefficients 로 고른다. (BT.709, FCC, BT.470BG, BT.601, SMPTE 240, BT.2020 NCL, 그 밖과 미지정은 BT.601)
//     Identity 는 G = Y, B = Cb, R = Cr 이고 YCgCo 는 H.273 식 47~50 이다.
//   - 범위: 제한 범위면 Y 는 (v - 16 x 2^(깊이-8)) / (219 x 2^(깊이-8)), Cb, Cr 은 (v - 2^(깊이-1)) / (224 x 2^(깊이-8)). 전체 범위면 최댓값으로 나눈다.
//   - 4:2:0, 4:2:2 의 크로마는 libavif 의 양선형(가장 가까운 것 9/16, 같은 줄과 같은 칸의 이웃 3/16 씩, 대각 1/16)으로 늘린다. 가장자리는 가장 가까운 것을 되풀이한다.
//   - 알파 그림은 전체 범위로 읽는다.
// - 원본(크롬)과 다른 것: 색 프로필(ICC, nclx 의 원색과 전달 특성) 변환을 하지 않는다. (image 크레이트로 푸는 PNG, JPEG 도 색 프로필을 걸지 않는다)
//   움직이는 AVIF(avis)는 정지 그림 항목(pitm)만 푼다. (브라우저는 움직인다) 게인 맵(HDR)은 쓰지 않는다.
// - rav1d 의 asm 은 끈 채 쓴다. (Cargo.toml)
//==============================================================================
use crate::core::image::Image;
use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::headers::{DAV1D_MC_BT2020_NCL, DAV1D_MC_BT470BG, DAV1D_MC_BT601, DAV1D_MC_BT709, DAV1D_MC_FCC, DAV1D_MC_IDENTITY, DAV1D_MC_SMPTE240, DAV1D_MC_SMPTE_YCGCO, DAV1D_PIXEL_LAYOUT_I400, DAV1D_PIXEL_LAYOUT_I420, DAV1D_PIXEL_LAYOUT_I422};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_get_picture, dav1d_open, dav1d_picture_unref, dav1d_send_data};
use std::collections::HashMap;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::rc::Rc;

// 해독에 쓰는 스레드 수의 끝. (그림 한 장)
const MAXIMUM_DECODE_THREAD_COUNT: usize = 8;
// 알파 그림을 가리키는 보조 형. (libavif AVIF_URN_ALPHA0, AVIF_URN_ALPHA1)
const ALPHA_AUXILIARY_TYPE_LIST: [&str; 2] = ["urn:mpeg:mpegB:cicp:systems:auxiliary:alpha", "urn:mpeg:hevc:2015:auxid:1"];
// grid 의 타일 수 끝. (줄 수와 칸 수는 각각 256 까지)
const MAXIMUM_GRID_TILE_COUNT: usize = 256 * 256;

//==============================================================================
// 푼 AV1 그림 한 장. (rav1d 의 그림에서 평면을 꺼내 둔 것, grid 는 타일을 이어 붙인 것)
//==============================================================================
struct DecodedAv1Picture {
	width: usize,
	height: usize,
	bit_depth: u32,
	// 크로마를 가로, 세로로 줄인 단계. (4:2:0 은 (1, 1), 4:2:2 는 (1, 0), 4:4:4 는 (0, 0))
	chroma_shift: (usize, usize),
	is_monochrome: bool,
	matrix_coefficients: u32,
	is_full_range: bool,
	// Y, Cb, Cr 평면. (줄 사이 빈칸 없이, 크로마는 줄인 크기, 흑백이면 Cb, Cr 은 비어 있다)
	plane_list: [Vec<u16>; 3],
}

impl DecodedAv1Picture {
	//==============================================================================
	// 크로마 평면의 너비와 높이.
	//==============================================================================
	fn get_chroma_size(&self) -> (usize, usize) {
		((self.width + self.chroma_shift.0) >> self.chroma_shift.0, (self.height + self.chroma_shift.1) >> self.chroma_shift.1)
	}

	//==============================================================================
	// 평면 하나의 너비와 높이. (0 은 Y, 1 과 2 는 크로마)
	//==============================================================================
	fn get_plane_size(&self, plane_index: usize) -> (usize, usize) {
		if plane_index == 0 {
			(self.width, self.height)
		}
		else {
			self.get_chroma_size()
		}
	}
}

//==============================================================================
// 항목 자료의 자리. (iloc 의 항목 하나)
//==============================================================================
struct HeifItemLocation {
	// 0 은 파일 안, 1 은 idat 안. (그 밖은 받지 않는다)
	construction_method: u16,
	base_offset: u64,
	// (자리, 길이) 목록. 길이 0 은 끝까지다.
	extent_list: Vec<(u64, u64)>,
}

//==============================================================================
// 항목 사이의 가리킴. (iref 의 갈래 하나)
//==============================================================================
struct HeifItemReference {
	reference_type: [u8; 4],
	from_item_id: u32,
	to_item_id_list: Vec<u32>,
}

//==============================================================================
// 항목 성질. (ipco 의 상자 하나, 쓰는 것만)
//==============================================================================
enum HeifItemProperty {
	// auxC 의 보조 형.
	AuxiliaryType(String),
	// irot 의 angle. (반시계 angle x 90 도)
	Rotation(u8),
	// imir 의 axis. (0 은 위아래, 1 은 좌우를 바꾼다)
	Mirror(u8),
	// colr 의 nclx. (matrix_coefficients, full_range_flag)
	ColorInformation {
		matrix_coefficients: u32,
		is_full_range: bool,
	},
	// clap 의 너비, 높이, 가로 자리, 세로 자리. (분자, 분모, 자리는 부호 있는 분자)
	CleanAperture {
		width: (u32, u32),
		height: (u32, u32),
		horizontal_offset: (i32, u32),
		vertical_offset: (i32, u32),
	},
	Other,
}

//==============================================================================
// meta 상자에서 읽은 것.
//==============================================================================
struct HeifMeta<'a> {
	primary_item_id: u32,
	item_type_table: HashMap<u32, [u8; 4]>,
	item_location_table: HashMap<u32, HeifItemLocation>,
	reference_list: Vec<HeifItemReference>,
	// ipco 의 성질. (ipma 의 번호는 1 부터, 0 은 없음)
	property_list: Vec<HeifItemProperty>,
	// 항목마다 붙은 성질 번호. (ipma 의 차례)
	association_table: HashMap<u32, Vec<usize>>,
	item_data: &'a [u8],
}

//==============================================================================
// 상자 안을 앞에서부터 읽는 것. (큰 쪽부터 읽는 정수)
//==============================================================================
struct BoxReader<'a> {
	bytes: &'a [u8],
	position: usize,
}

impl<'a> BoxReader<'a> {
	//==============================================================================
	// 생성.
	//==============================================================================
	fn new(bytes: &'a [u8]) -> BoxReader<'a> {
		BoxReader {
			bytes,
			position: 0,
		}
	}

	//==============================================================================
	// 바이트 몇 개 읽기. (모자라면 Err)
	//==============================================================================
	fn read_bytes(&mut self, byte_count: usize) -> Result<&'a [u8], String> {
		let end = self.position.checked_add(byte_count).filter(|end| *end <= self.bytes.len()).ok_or_else(|| String::from("AVIF 상자가 짧습니다."))?;
		let read_bytes = &self.bytes[self.position..end];
		self.position = end;
		Ok(read_bytes)
	}

	//==============================================================================
	// 정수 읽기. (byte_count 바이트, 0 이면 0)
	//==============================================================================
	fn read_unsigned(&mut self, byte_count: usize) -> Result<u64, String> {
		let read_bytes = self.read_bytes(byte_count)?;
		Ok(read_bytes.iter().fold(0u64, |value, byte| (value << 8) | *byte as u64))
	}

	//==============================================================================
	// 8, 16, 32 비트 정수 읽기.
	//==============================================================================
	fn read_u8(&mut self) -> Result<u8, String> {
		Ok(self.read_unsigned(1)? as u8)
	}

	fn read_u16(&mut self) -> Result<u16, String> {
		Ok(self.read_unsigned(2)? as u16)
	}

	fn read_u32(&mut self) -> Result<u32, String> {
		Ok(self.read_unsigned(4)? as u32)
	}

	//==============================================================================
	// 네 글자 이름 읽기.
	//==============================================================================
	fn read_four_character_code(&mut self) -> Result<[u8; 4], String> {
		let read_bytes = self.read_bytes(4)?;
		Ok([read_bytes[0], read_bytes[1], read_bytes[2], read_bytes[3]])
	}

	//==============================================================================
	// 남은 바이트.
	//==============================================================================
	fn read_remaining(&mut self) -> &'a [u8] {
		let remaining_bytes = &self.bytes[self.position..];
		self.position = self.bytes.len();
		remaining_bytes
	}
}

//==============================================================================
// 상자 목록으로 나누기. ((이름, 내용) 목록, 크기 1 은 64 비트 크기, 0 은 끝까지)
//==============================================================================
fn split_boxes(bytes: &[u8]) -> Result<Vec<([u8; 4], &[u8])>, String> {
	let mut box_list: Vec<([u8; 4], &[u8])> = Vec::new();
	let mut reader = BoxReader::new(bytes);
	while bytes.len() - reader.position >= 8 {
		let box_start = reader.position;
		let mut box_size = reader.read_u32()? as u64;
		let box_type = reader.read_four_character_code()?;
		if box_size == 1 {
			box_size = reader.read_unsigned(8)?;
		}
		else if box_size == 0 {
			box_size = (bytes.len() - box_start) as u64;
		}
		let content_start = reader.position;
		let box_end = box_start.checked_add(box_size as usize).filter(|box_end| *box_end <= bytes.len() && *box_end >= content_start).ok_or_else(|| String::from("AVIF 상자 크기가 맞지 않습니다."))?;
		box_list.push((box_type, &bytes[content_start..box_end]));
		reader.position = box_end;
	}
	Ok(box_list)
}

//==============================================================================
// 버전과 표시가 붙은 상자의 머리 나누기. ((버전, 표시, 나머지))
//==============================================================================
fn split_full_box(content: &[u8]) -> Result<(u8, u32, &[u8]), String> {
	if content.len() < 4 {
		return Err(String::from("AVIF 상자가 짧습니다."));
	}
	let flags = ((content[1] as u32) << 16) | ((content[2] as u32) << 8) | content[3] as u32;
	Ok((content[0], flags, &content[4..]))
}

//==============================================================================
// meta 상자 읽기. (pitm, iinf, iloc, iref, iprp, idat)
//==============================================================================
fn read_heif_meta(file_bytes: &[u8]) -> Result<HeifMeta<'_>, String> {
	let meta_content = split_boxes(file_bytes)?.into_iter().find(|(box_type, _)| box_type == b"meta").map(|(_, content)| content).ok_or_else(|| String::from("AVIF 에 meta 상자가 없습니다."))?;
	let (_, _, meta_body) = split_full_box(meta_content)?;
	let mut meta = HeifMeta {
		primary_item_id: 0,
		item_type_table: HashMap::new(),
		item_location_table: HashMap::new(),
		reference_list: Vec::new(),
		property_list: Vec::new(),
		association_table: HashMap::new(),
		item_data: &[],
	};
	let mut has_primary_item = false;
	for (box_type, content) in split_boxes(meta_body)? {
		match &box_type {
			b"pitm" => {
				let (version, _, body) = split_full_box(content)?;
				let mut reader = BoxReader::new(body);
				meta.primary_item_id = if version == 0 { reader.read_u16()? as u32 } else { reader.read_u32()? };
				has_primary_item = true;
			}
			b"iinf" => {
				read_item_info_box(content, &mut meta)?;
			}
			b"iloc" => {
				read_item_location_box(content, &mut meta)?;
			}
			b"iref" => {
				read_item_reference_box(content, &mut meta)?;
			}
			b"iprp" => {
				read_item_property_box(content, &mut meta)?;
			}
			b"idat" => {
				meta.item_data = content;
			}
			_ => {
			}
		}
	}
	if !has_primary_item {
		return Err(String::from("AVIF 에 주 그림(pitm)이 없습니다."));
	}
	Ok(meta)
}

//==============================================================================
// iinf 읽기. (infe 판 2, 3 의 항목 형)
//==============================================================================
fn read_item_info_box<'a>(content: &'a [u8], meta: &mut HeifMeta<'a>) -> Result<(), String> {
	let (version, _, body) = split_full_box(content)?;
	let mut reader = BoxReader::new(body);
	if version == 0 {
		reader.read_u16()?;
	}
	else {
		reader.read_u32()?;
	}
	for (box_type, entry_content) in split_boxes(reader.read_remaining())? {
		if &box_type != b"infe" {
			continue;
		}
		let (entry_version, _, entry_body) = split_full_box(entry_content)?;
		if entry_version < 2 {
			continue;
		}
		let mut entry_reader = BoxReader::new(entry_body);
		let item_id = if entry_version == 2 { entry_reader.read_u16()? as u32 } else { entry_reader.read_u32()? };
		entry_reader.read_u16()?;
		let item_type = entry_reader.read_four_character_code()?;
		meta.item_type_table.insert(item_id, item_type);
	}
	Ok(())
}

//==============================================================================
// iloc 읽기. (판 0, 1, 2, 자리와 길이의 바이트 수는 머리에 적힌 대로)
//==============================================================================
fn read_item_location_box<'a>(content: &'a [u8], meta: &mut HeifMeta<'a>) -> Result<(), String> {
	let (version, _, body) = split_full_box(content)?;
	let mut reader = BoxReader::new(body);
	let size_byte = reader.read_u8()?;
	let offset_size = (size_byte >> 4) as usize;
	let length_size = (size_byte & 0x0f) as usize;
	let index_byte = reader.read_u8()?;
	let base_offset_size = (index_byte >> 4) as usize;
	let index_size = if version == 1 || version == 2 { (index_byte & 0x0f) as usize } else { 0 };
	let item_count = if version < 2 { reader.read_u16()? as u32 } else { reader.read_u32()? };
	for _ in 0..item_count {
		let item_id = if version < 2 { reader.read_u16()? as u32 } else { reader.read_u32()? };
		let construction_method = if version == 1 || version == 2 { reader.read_u16()? & 0x0f } else { 0 };
		reader.read_u16()?;
		let base_offset = reader.read_unsigned(base_offset_size)?;
		let extent_count = reader.read_u16()?;
		let mut extent_list: Vec<(u64, u64)> = Vec::with_capacity(extent_count as usize);
		for _ in 0..extent_count {
			reader.read_unsigned(index_size)?;
			let extent_offset = reader.read_unsigned(offset_size)?;
			let extent_length = reader.read_unsigned(length_size)?;
			extent_list.push((extent_offset, extent_length));
		}
		meta.item_location_table.insert(item_id, HeifItemLocation {
			construction_method,
			base_offset,
			extent_list,
		});
	}
	Ok(())
}

//==============================================================================
// iref 읽기. (판 0 은 16 비트, 1 은 32 비트 항목 번호)
//==============================================================================
fn read_item_reference_box<'a>(content: &'a [u8], meta: &mut HeifMeta<'a>) -> Result<(), String> {
	let (version, _, body) = split_full_box(content)?;
	for (reference_type, reference_content) in split_boxes(body)? {
		let mut reader = BoxReader::new(reference_content);
		let from_item_id = if version == 0 { reader.read_u16()? as u32 } else { reader.read_u32()? };
		let reference_count = reader.read_u16()?;
		let mut to_item_id_list: Vec<u32> = Vec::with_capacity(reference_count as usize);
		for _ in 0..reference_count {
			to_item_id_list.push(if version == 0 { reader.read_u16()? as u32 } else { reader.read_u32()? });
		}
		meta.reference_list.push(HeifItemReference {
			reference_type,
			from_item_id,
			to_item_id_list,
		});
	}
	Ok(())
}

//==============================================================================
// iprp 읽기. (ipco 의 성질과 ipma 의 항목마다 붙은 성질 번호)
//==============================================================================
fn read_item_property_box<'a>(content: &'a [u8], meta: &mut HeifMeta<'a>) -> Result<(), String> {
	for (box_type, property_content) in split_boxes(content)? {
		if &box_type == b"ipco" {
			for (property_type, property_body) in split_boxes(property_content)? {
				meta.property_list.push(read_item_property(&property_type, property_body)?);
			}
		}
		else if &box_type == b"ipma" {
			let (version, flags, body) = split_full_box(property_content)?;
			let mut reader = BoxReader::new(body);
			let entry_count = reader.read_u32()?;
			for _ in 0..entry_count {
				let item_id = if version < 1 { reader.read_u16()? as u32 } else { reader.read_u32()? };
				let association_count = reader.read_u8()?;
				let association_list = meta.association_table.entry(item_id).or_default();
				for _ in 0..association_count {
					let property_index = if (flags & 1) != 0 { (reader.read_u16()? & 0x7fff) as usize } else { (reader.read_u8()? & 0x7f) as usize };
					association_list.push(property_index);
				}
			}
		}
	}
	Ok(())
}

//==============================================================================
// 성질 상자 하나 읽기. (auxC, colr 의 nclx, irot, imir, clap, 그 밖은 Other)
//==============================================================================
fn read_item_property(property_type: &[u8; 4], property_body: &[u8]) -> Result<HeifItemProperty, String> {
	match property_type {
		b"colr" => {
			let mut reader = BoxReader::new(property_body);
			if &reader.read_four_character_code()? != b"nclx" {
				return Ok(HeifItemProperty::Other);
			}
			reader.read_u16()?;
			reader.read_u16()?;
			let matrix_coefficients = reader.read_u16()? as u32;
			let is_full_range = (reader.read_u8()? & 0x80) != 0;
			Ok(HeifItemProperty::ColorInformation {
				matrix_coefficients,
				is_full_range,
			})
		}
		b"auxC" => {
			let (_, _, body) = split_full_box(property_body)?;
			let type_bytes = body.split(|byte| *byte == 0).next().unwrap_or(&[]);
			Ok(HeifItemProperty::AuxiliaryType(String::from_utf8_lossy(type_bytes).into_owned()))
		}
		b"irot" => {
			let mut reader = BoxReader::new(property_body);
			Ok(HeifItemProperty::Rotation(reader.read_u8()? & 0x03))
		}
		b"imir" => {
			let mut reader = BoxReader::new(property_body);
			Ok(HeifItemProperty::Mirror(reader.read_u8()? & 0x01))
		}
		b"clap" => {
			let mut reader = BoxReader::new(property_body);
			let width = (reader.read_u32()?, reader.read_u32()?);
			let height = (reader.read_u32()?, reader.read_u32()?);
			let horizontal_offset = (reader.read_u32()? as i32, reader.read_u32()?);
			let vertical_offset = (reader.read_u32()? as i32, reader.read_u32()?);
			Ok(HeifItemProperty::CleanAperture {
				width,
				height,
				horizontal_offset,
				vertical_offset,
			})
		}
		_ => {
			Ok(HeifItemProperty::Other)
		}
	}
}

impl HeifMeta<'_> {
	//==============================================================================
	// 항목에 붙은 성질 목록. (ipma 의 차례)
	//==============================================================================
	fn get_item_property_list(&self, item_id: u32) -> Vec<&HeifItemProperty> {
		match self.association_table.get(&item_id) {
			Some(association_list) => {
				association_list.iter().filter(|property_index| **property_index > 0).filter_map(|property_index| self.property_list.get(*property_index - 1)).collect()
			}
			None => {
				Vec::new()
			}
		}
	}

	//==============================================================================
	// 항목 자료 읽기. (iloc 의 조각을 차례로 이어 붙인다, 파일 안과 idat 안)
	//==============================================================================
	fn read_item_data(&self, file_bytes: &[u8], item_id: u32) -> Result<Vec<u8>, String> {
		let item_location = self.item_location_table.get(&item_id).ok_or_else(|| format!("AVIF 항목 {} 의 자리가 없습니다.", item_id))?;
		let source_bytes = match item_location.construction_method {
			0 => {
				file_bytes
			}
			1 => {
				self.item_data
			}
			_ => {
				return Err(String::from("AVIF 항목 자리의 만드는 방법을 모릅니다."));
			}
		};
		let mut item_bytes: Vec<u8> = Vec::new();
		for (extent_offset, extent_length) in item_location.extent_list.iter() {
			let start = item_location.base_offset.checked_add(*extent_offset).map(|start| start as usize).filter(|start| *start <= source_bytes.len()).ok_or_else(|| String::from("AVIF 항목 자리가 파일 밖입니다."))?;
			let end = if *extent_length == 0 {
				source_bytes.len()
			}
			else {
				start.checked_add(*extent_length as usize).filter(|end| *end <= source_bytes.len()).ok_or_else(|| String::from("AVIF 항목 길이가 파일 밖입니다."))?
			};
			item_bytes.extend_from_slice(&source_bytes[start..end]);
		}
		Ok(item_bytes)
	}

	//==============================================================================
	// 주 그림의 알파 항목. (주 그림을 auxl 로 가리키고 auxC 가 알파 형인 항목)
	//==============================================================================
	fn find_alpha_item_id(&self) -> Option<u32> {
		self.reference_list.iter().filter(|reference| &reference.reference_type == b"auxl" && reference.from_item_id != self.primary_item_id && reference.to_item_id_list.contains(&self.primary_item_id)).map(|reference| reference.from_item_id).find(|item_id| {
			self.get_item_property_list(*item_id).iter().any(|property| match property {
				HeifItemProperty::AuxiliaryType(auxiliary_type) => {
					ALPHA_AUXILIARY_TYPE_LIST.contains(&auxiliary_type.as_str())
				}
				_ => {
					false
				}
			})
		})
	}

	//==============================================================================
	// 주 그림이 알파로 곱해졌는지. (주 그림에서 알파 항목으로 가는 prem)
	//==============================================================================
	fn is_premultiplied(&self, alpha_item_id: u32) -> bool {
		self.reference_list.iter().any(|reference| &reference.reference_type == b"prem" && reference.from_item_id == self.primary_item_id && reference.to_item_id_list.contains(&alpha_item_id))
	}

	//==============================================================================
	// 항목 하나를 YUV 그림으로 풀기. (av01 은 그대로, grid 는 타일을 풀어 이어 붙인다)
	//==============================================================================
	fn decode_item_picture(&self, file_bytes: &[u8], item_id: u32) -> Result<DecodedAv1Picture, String> {
		let item_type = self.item_type_table.get(&item_id).copied().ok_or_else(|| format!("AVIF 항목 {} 의 형이 없습니다.", item_id))?;
		match &item_type {
			b"av01" => {
				decode_av1_picture(&self.read_item_data(file_bytes, item_id)?)
			}
			b"grid" => {
				self.decode_grid_picture(file_bytes, item_id)
			}
			_ => {
				Err(format!("AVIF 항목 형 {} 을 모릅니다.", String::from_utf8_lossy(&item_type)))
			}
		}
	}

	//==============================================================================
	// grid 항목 풀기. (ImageGrid 의 줄, 칸, 출력 크기를 읽고 dimg 차례의 타일을 왼쪽 위부터 줄마다 놓은 뒤 출력 크기로 자른다)
	// - 타일은 모두 같은 크기, 깊이, 크로마 배치여야 한다. (libavif 와 같다)
	//==============================================================================
	fn decode_grid_picture(&self, file_bytes: &[u8], item_id: u32) -> Result<DecodedAv1Picture, String> {
		let grid_bytes = self.read_item_data(file_bytes, item_id)?;
		let mut reader = BoxReader::new(&grid_bytes);
		if reader.read_u8()? != 0 {
			return Err(String::from("AVIF grid 의 판을 모릅니다."));
		}
		let flags = reader.read_u8()?;
		let row_count = reader.read_u8()? as usize + 1;
		let column_count = reader.read_u8()? as usize + 1;
		let field_size = if (flags & 1) != 0 { 4 } else { 2 };
		let output_width = reader.read_unsigned(field_size)? as usize;
		let output_height = reader.read_unsigned(field_size)? as usize;
		if output_width == 0 || output_height == 0 {
			return Err(String::from("AVIF grid 의 크기가 0 입니다."));
		}
		let tile_item_id_list: Vec<u32> = self.reference_list.iter().filter(|reference| &reference.reference_type == b"dimg" && reference.from_item_id == item_id).flat_map(|reference| reference.to_item_id_list.iter().copied()).collect();
		let tile_count = row_count * column_count;
		if tile_count > MAXIMUM_GRID_TILE_COUNT || tile_item_id_list.len() != tile_count {
			return Err(String::from("AVIF grid 의 타일 수가 맞지 않습니다."));
		}
		let mut tile_picture_list: Vec<DecodedAv1Picture> = Vec::with_capacity(tile_count);
		for tile_item_id in tile_item_id_list.iter() {
			if &self.item_type_table.get(tile_item_id).copied().unwrap_or([0; 4]) != b"av01" {
				return Err(String::from("AVIF grid 의 타일이 av01 이 아닙니다."));
			}
			tile_picture_list.push(decode_av1_picture(&self.read_item_data(file_bytes, *tile_item_id)?)?);
		}
		let first_tile = &tile_picture_list[0];
		let is_same_tile = |tile: &DecodedAv1Picture| -> bool {
			tile.width == first_tile.width && tile.height == first_tile.height && tile.bit_depth == first_tile.bit_depth && tile.chroma_shift == first_tile.chroma_shift && tile.is_monochrome == first_tile.is_monochrome
		};
		if !tile_picture_list.iter().all(is_same_tile) {
			return Err(String::from("AVIF grid 의 타일 꼴이 서로 다릅니다."));
		}
		if first_tile.width * column_count < output_width || first_tile.height * row_count < output_height {
			return Err(String::from("AVIF grid 의 타일이 출력 크기를 덮지 못합니다."));
		}
		let mut grid_picture = DecodedAv1Picture {
			width: output_width,
			height: output_height,
			bit_depth: first_tile.bit_depth,
			chroma_shift: first_tile.chroma_shift,
			is_monochrome: first_tile.is_monochrome,
			matrix_coefficients: first_tile.matrix_coefficients,
			is_full_range: first_tile.is_full_range,
			plane_list: [Vec::new(), Vec::new(), Vec::new()],
		};
		let plane_count = if grid_picture.is_monochrome { 1 } else { 3 };
		for plane_index in 0..plane_count {
			let (grid_plane_width, grid_plane_height) = grid_picture.get_plane_size(plane_index);
			let (tile_plane_width, tile_plane_height) = first_tile.get_plane_size(plane_index);
			let mut grid_plane = vec![0u16; grid_plane_width * grid_plane_height];
			for (tile_index, tile_picture) in tile_picture_list.iter().enumerate() {
				let tile_left = (tile_index % column_count) * tile_plane_width;
				let tile_top = (tile_index / column_count) * tile_plane_height;
				let tile_plane = &tile_picture.plane_list[plane_index];
				for tile_row in 0..tile_plane_height {
					let grid_row = tile_top + tile_row;
					if grid_row >= grid_plane_height || tile_left >= grid_plane_width {
						continue;
					}
					let copy_width = tile_plane_width.min(grid_plane_width - tile_left);
					let source_start = tile_row * tile_plane_width;
					let target_start = grid_row * grid_plane_width + tile_left;
					grid_plane[target_start..target_start + copy_width].copy_from_slice(&tile_plane[source_start..source_start + copy_width]);
				}
			}
			grid_picture.plane_list[plane_index] = grid_plane;
		}
		Ok(grid_picture)
	}
}

//==============================================================================
// AVIF 풀기. (주 그림을 RGB 로, 알파 그림이 있으면 알파로 만들고 clap, irot, imir 를 건다. 못 풀면 까닭)
//==============================================================================
pub(crate) fn decode_avif(bytes: &[u8]) -> Result<Rc<Image>, String> {
	let meta = read_heif_meta(bytes)?;
	let mut color_picture = meta.decode_item_picture(bytes, meta.primary_item_id)?;
	let property_list = meta.get_item_property_list(meta.primary_item_id);
	for property in property_list.iter() {
		if let HeifItemProperty::ColorInformation { matrix_coefficients, is_full_range } = property {
			color_picture.matrix_coefficients = *matrix_coefficients;
			color_picture.is_full_range = *is_full_range;
		}
	}
	let alpha_item_id = meta.find_alpha_item_id();
	let alpha_picture = match alpha_item_id {
		Some(alpha_item_id) => {
			let alpha_picture = meta.decode_item_picture(bytes, alpha_item_id)?;
			if alpha_picture.width != color_picture.width || alpha_picture.height != color_picture.height {
				return Err(String::from("AVIF 알파 그림의 크기가 다릅니다."));
			}
			Some(alpha_picture)
		}
		None => {
			None
		}
	};
	let is_premultiplied = alpha_item_id.map(|alpha_item_id| meta.is_premultiplied(alpha_item_id)).unwrap_or(false);
	let mut pixels = convert_to_rgba_pixels(&color_picture, alpha_picture.as_ref());
	let mut width = color_picture.width;
	let mut height = color_picture.height;
	for property in property_list.iter() {
		if let HeifItemProperty::CleanAperture { width: clean_width, height: clean_height, horizontal_offset, vertical_offset } = property {
			if let Some((crop_width, crop_height)) = read_clean_aperture_size(width, height, *clean_width, *clean_height, *horizontal_offset, *vertical_offset) {
				pixels = crop_pixels(&pixels, width, crop_width, crop_height);
				width = crop_width;
				height = crop_height;
			}
		}
	}
	for property in property_list.iter() {
		if let HeifItemProperty::Rotation(angle) = property {
			for _ in 0..*angle {
				pixels = rotate_pixels_anticlockwise(&pixels, width, height);
				std::mem::swap(&mut width, &mut height);
			}
		}
	}
	for property in property_list.iter() {
		if let HeifItemProperty::Mirror(axis) = property {
			mirror_pixels(&mut pixels, width, height, *axis);
		}
	}
	if alpha_picture.is_some() && is_premultiplied {
		Ok(Image::create_from_premultiplied_pixels(width as u32, height as u32, pixels))
	}
	else {
		Ok(Image::create_from_pixels(width as u32, height as u32, pixels))
	}
}

//==============================================================================
// clap 의 자를 크기. (크롬 AVIFImageDecoder 처럼 자른 자리의 원점이 (0, 0) 이고 크기가 정수로 그림 안이면 Some, 아니면 그림 전체를 쓴다)
// - 원점 = (그림 크기 - 깨끗한 크기) / 2 + 자리. (ISO/IEC 14496-12 의 clap, 분수로 셈한다)
//==============================================================================
fn read_clean_aperture_size(image_width: usize, image_height: usize, clean_width: (u32, u32), clean_height: (u32, u32), horizontal_offset: (i32, u32), vertical_offset: (i32, u32)) -> Option<(usize, usize)> {
	let read_axis = |image_size: usize, clean_size: (u32, u32), offset: (i32, u32)| -> Option<usize> {
		if clean_size.1 == 0 || offset.1 == 0 || clean_size.0 % clean_size.1 != 0 {
			return None;
		}
		let crop_size = (clean_size.0 / clean_size.1) as i128;
		if crop_size <= 0 || crop_size > image_size as i128 {
			return None;
		}
		// 원점 x 2 x 분모 = (그림 - 깨끗한) x 분모 + 2 x 자리 분자
		let origin_numerator = (image_size as i128 - crop_size) * offset.1 as i128 + 2 * offset.0 as i128;
		if origin_numerator != 0 {
			return None;
		}
		Some(crop_size as usize)
	};
	let crop_width = read_axis(image_width, clean_width, horizontal_offset)?;
	let crop_height = read_axis(image_height, clean_height, vertical_offset)?;
	Some((crop_width, crop_height))
}

//==============================================================================
// RGBA 픽셀의 왼쪽 위 crop_width x crop_height 만 남기기.
//==============================================================================
fn crop_pixels(pixels: &[u8], width: usize, crop_width: usize, crop_height: usize) -> Vec<u8> {
	let mut cropped_pixels: Vec<u8> = Vec::with_capacity(crop_width * crop_height * 4);
	for row in 0..crop_height {
		let row_start = row * width * 4;
		cropped_pixels.extend_from_slice(&pixels[row_start..row_start + crop_width * 4]);
	}
	cropped_pixels
}

//==============================================================================
// RGBA 픽셀을 반시계로 90 도 돌리기. (새 너비는 옛 높이)
//==============================================================================
fn rotate_pixels_anticlockwise(pixels: &[u8], width: usize, height: usize) -> Vec<u8> {
	let mut rotated_pixels = vec![0u8; pixels.len()];
	for row in 0..height {
		for column in 0..width {
			// 옛 (column, row) 는 새 (row, width - 1 - column) 으로 간다. (새 너비 = height)
			let source_index = (row * width + column) * 4;
			let target_index = ((width - 1 - column) * height + row) * 4;
			rotated_pixels[target_index..target_index + 4].copy_from_slice(&pixels[source_index..source_index + 4]);
		}
	}
	rotated_pixels
}

//==============================================================================
// RGBA 픽셀 뒤집기. (axis 0 은 위아래를, 1 은 좌우를 바꾼다)
//==============================================================================
fn mirror_pixels(pixels: &mut [u8], width: usize, height: usize, axis: u8) {
	if axis == 0 {
		for row in 0..height / 2 {
			for byte_index in 0..width * 4 {
				pixels.swap(row * width * 4 + byte_index, (height - 1 - row) * width * 4 + byte_index);
			}
		}
	}
	else {
		for row in 0..height {
			for column in 0..width / 2 {
				for channel in 0..4 {
					pixels.swap((row * width + column) * 4 + channel, (row * width + width - 1 - column) * 4 + channel);
				}
			}
		}
	}
}

//==============================================================================
// AV1 자료 하나(AVIF 항목 하나)를 그림으로 풀기. (여러 층이면 마지막 층이 다 된 그림이다)
//==============================================================================
fn decode_av1_picture(av1_bytes: &[u8]) -> Result<DecodedAv1Picture, String> {
	let mut context: Option<Dav1dContext> = None;
	unsafe {
		let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
		dav1d_default_settings(NonNull::from(&mut settings).cast());
		let settings = settings.assume_init_mut();
		let thread_count = std::thread::available_parallelism().map(|thread_count| thread_count.get()).unwrap_or(1).min(MAXIMUM_DECODE_THREAD_COUNT);
		settings.n_threads = thread_count as i32;
		// 그림 한 장이라 미리 풀어 둘 것이 없다. 가장 좋은 층만 낸다. (libavif 와 같다)
		settings.max_frame_delay = 1;
		settings.all_layers = 0;
		let open_result = dav1d_open(Some(NonNull::from(&mut context)), Some(NonNull::from(settings)));
		if open_result.0 != 0 {
			return Err(format!("AV1 해독기를 열지 못했습니다. ({})", open_result.0));
		}
	}
	let decode_result = unsafe { decode_av1_picture_with_context(context, av1_bytes) };
	unsafe {
		dav1d_close(Some(NonNull::from(&mut context)));
	}
	decode_result
}

//==============================================================================
// 열린 해독기로 AV1 자료 하나 풀기. (자료를 다 보낼 때까지 나온 그림을 꺼내고, 끝에 남은 그림까지 꺼내 마지막 것을 쓴다)
//==============================================================================
unsafe fn decode_av1_picture_with_context(context: Option<Dav1dContext>, av1_bytes: &[u8]) -> Result<DecodedAv1Picture, String> {
	let mut data = Dav1dData::default();
	let buffer = unsafe { dav1d_data_create(Some(NonNull::from(&mut data)), av1_bytes.len()) };
	if buffer.is_null() {
		return Err(String::from("AV1 자료를 담을 곳을 만들지 못했습니다."));
	}
	unsafe {
		std::ptr::copy_nonoverlapping(av1_bytes.as_ptr(), buffer, av1_bytes.len());
	}
	let mut last_picture: Option<Dav1dPicture> = None;
	let mut send_error: Option<i32> = None;
	loop {
		let send_result = unsafe { dav1d_send_data(context, Some(NonNull::from(&mut data))) };
		if send_result.0 == 0 {
			break;
		}
		// 해독기가 그림을 들고 있어 자료를 더 받지 못하면 그림을 꺼내고 다시 보낸다.
		let mut pending_picture = Dav1dPicture::default();
		let get_result = unsafe { dav1d_get_picture(context, Some(NonNull::from(&mut pending_picture))) };
		if get_result.0 != 0 {
			send_error = Some(send_result.0);
			break;
		}
		if let Some(mut previous_picture) = last_picture.replace(pending_picture) {
			unsafe {
				dav1d_picture_unref(Some(NonNull::from(&mut previous_picture)));
			}
		}
	}
	unsafe {
		dav1d_data_unref(Some(NonNull::from(&mut data)));
	}
	if let Some(send_error) = send_error {
		if let Some(mut previous_picture) = last_picture.take() {
			unsafe {
				dav1d_picture_unref(Some(NonNull::from(&mut previous_picture)));
			}
		}
		return Err(format!("AV1 자료를 보내지 못했습니다. ({})", send_error));
	}
	loop {
		let mut picture = Dav1dPicture::default();
		let get_result = unsafe { dav1d_get_picture(context, Some(NonNull::from(&mut picture))) };
		if get_result.0 != 0 {
			break;
		}
		if let Some(mut previous_picture) = last_picture.replace(picture) {
			unsafe {
				dav1d_picture_unref(Some(NonNull::from(&mut previous_picture)));
			}
		}
	}
	let mut picture = last_picture.ok_or_else(|| String::from("AV1 그림이 나오지 않았습니다."))?;
	let decoded_picture = unsafe { copy_decoded_picture(&picture) };
	unsafe {
		dav1d_picture_unref(Some(NonNull::from(&mut picture)));
	}
	decoded_picture
}

//==============================================================================
// rav1d 그림의 평면을 꺼내 두기. (8 비트는 바이트, 그보다 깊으면 16 비트 낱개, 줄 너비(stride)는 Y 하나와 크로마 둘이 함께 쓰는 하나)
//==============================================================================
unsafe fn copy_decoded_picture(picture: &Dav1dPicture) -> Result<DecodedAv1Picture, String> {
	let width = picture.p.w.max(0) as usize;
	let height = picture.p.h.max(0) as usize;
	if width == 0 || height == 0 {
		return Err(String::from("AV1 그림의 크기가 0 입니다."));
	}
	let bit_depth = picture.p.bpc.max(8) as u32;
	let layout = picture.p.layout;
	let is_monochrome = layout == DAV1D_PIXEL_LAYOUT_I400;
	let chroma_shift = if layout == DAV1D_PIXEL_LAYOUT_I420 {
		(1, 1)
	}
	else if layout == DAV1D_PIXEL_LAYOUT_I422 {
		(1, 0)
	}
	else {
		(0, 0)
	};
	let (matrix_coefficients, is_full_range) = match picture.seq_hdr {
		Some(sequence_header) => {
			let sequence_header = unsafe { sequence_header.as_ref() };
			(sequence_header.mtrx, sequence_header.color_range != 0)
		}
		None => {
			(DAV1D_MC_BT601, true)
		}
	};
	let mut decoded_picture = DecodedAv1Picture {
		width,
		height,
		bit_depth,
		chroma_shift,
		is_monochrome,
		matrix_coefficients,
		is_full_range,
		plane_list: [Vec::new(), Vec::new(), Vec::new()],
	};
	let (chroma_width, chroma_height) = decoded_picture.get_chroma_size();
	let plane_count = if is_monochrome { 1 } else { 3 };
	for plane_index in 0..plane_count {
		let (plane_width, plane_height) = if plane_index == 0 {
			(width, height)
		}
		else {
			(chroma_width, chroma_height)
		};
		let plane_data = picture.data[plane_index].ok_or_else(|| String::from("AV1 그림의 평면이 없습니다."))?;
		let stride_bytes = picture.stride[usize::from(plane_index != 0)];
		if stride_bytes <= 0 {
			return Err(String::from("AV1 그림의 줄 너비가 0 입니다."));
		}
		let stride_bytes = stride_bytes as usize;
		let mut plane_values: Vec<u16> = Vec::with_capacity(plane_width * plane_height);
		for row_index in 0..plane_height {
			let row_pointer = unsafe { (plane_data.as_ptr() as *const u8).add(row_index * stride_bytes) };
			if bit_depth <= 8 {
				let row_bytes = unsafe { std::slice::from_raw_parts(row_pointer, plane_width) };
				plane_values.extend(row_bytes.iter().map(|value| *value as u16));
			}
			else {
				let row_values = unsafe { std::slice::from_raw_parts(row_pointer as *const u16, plane_width) };
				plane_values.extend_from_slice(row_values);
			}
		}
		decoded_picture.plane_list[plane_index] = plane_values;
	}
	Ok(decoded_picture)
}

//==============================================================================
// 색 계수 Kr, Kb. (libavif avifCalcYUVCoefficients, 표에 없는 것과 미지정은 BT.601)
//==============================================================================
fn read_luma_coefficients(matrix_coefficients: u32) -> (f32, f32) {
	match matrix_coefficients {
		DAV1D_MC_BT709 => {
			(0.2126, 0.0722)
		}
		DAV1D_MC_FCC => {
			(0.30, 0.11)
		}
		DAV1D_MC_BT470BG | DAV1D_MC_BT601 => {
			(0.299, 0.114)
		}
		DAV1D_MC_SMPTE240 => {
			(0.212, 0.087)
		}
		DAV1D_MC_BT2020_NCL => {
			(0.2627, 0.0593)
		}
		_ => {
			(0.299, 0.114)
		}
	}
}

//==============================================================================
// 4:2:0, 4:2:2 의 크로마 하나 읽기. (libavif 의 양선형 9/16, 3/16, 3/16, 1/16, 4:4:4 는 그 자리 값)
// - 가장 가까운 크로마 칸의 이웃은 홀수 칸이면 오른쪽(아래), 짝수 칸이면 왼쪽(위)이다. 첫 칸과 마지막 홀수 칸은 이웃 대신 자기를 쓴다. 4:2:2 는 세로 이웃이 없다.
//==============================================================================
fn read_upsampled_chroma(picture: &DecodedAv1Picture, plane: &[u16], column: usize, row: usize) -> f32 {
	let (chroma_width, _) = picture.get_chroma_size();
	let chroma_column = column >> picture.chroma_shift.0;
	let chroma_row = row >> picture.chroma_shift.1;
	let read_value = |read_column: usize, read_row: usize| -> f32 {
		plane[read_row * chroma_width + read_column] as f32
	};
	if picture.chroma_shift == (0, 0) {
		return read_value(chroma_column, chroma_row);
	}
	let adjacent_column = if column == 0 || (column == picture.width - 1 && column % 2 != 0) {
		chroma_column
	}
	else if column % 2 != 0 {
		chroma_column + 1
	}
	else {
		chroma_column - 1
	};
	let adjacent_row = if picture.chroma_shift.1 == 0 || row == 0 || (row == picture.height - 1 && row % 2 != 0) {
		chroma_row
	}
	else if row % 2 != 0 {
		chroma_row + 1
	}
	else {
		chroma_row - 1
	};
	read_value(chroma_column, chroma_row) * (9.0 / 16.0) + read_value(adjacent_column, chroma_row) * (3.0 / 16.0) + read_value(chroma_column, adjacent_row) * (3.0 / 16.0) + read_value(adjacent_column, adjacent_row) * (1.0 / 16.0)
}

//==============================================================================
// YUV 를 8 비트 RGBA 로. (libavif avifImageYUVToRGB 의 기준 셈, 알파 그림이 없으면 불투명)
//==============================================================================
fn convert_to_rgba_pixels(picture: &DecodedAv1Picture, alpha_picture: Option<&DecodedAv1Picture>) -> Vec<u8> {
	let maximum_value = ((1u32 << picture.bit_depth) - 1) as f32;
	let depth_scale = (1u32 << (picture.bit_depth - 8)) as f32;
	let (bias_y, range_y, range_uv) = if picture.is_full_range {
		(0.0, maximum_value, maximum_value)
	}
	else {
		(16.0 * depth_scale, 219.0 * depth_scale, 224.0 * depth_scale)
	};
	let bias_uv = (1u32 << (picture.bit_depth - 1)) as f32;
	let is_identity = picture.matrix_coefficients == DAV1D_MC_IDENTITY;
	let is_ycgco = picture.matrix_coefficients == DAV1D_MC_SMPTE_YCGCO;
	let (luma_red, luma_blue) = read_luma_coefficients(picture.matrix_coefficients);
	let luma_green = 1.0 - luma_red - luma_blue;
	let alpha_maximum_value = alpha_picture.map(|alpha_picture| ((1u32 << alpha_picture.bit_depth) - 1) as f32).unwrap_or(1.0);
	let to_byte = |value: f32| -> u8 {
		(value.clamp(0.0, 1.0) * 255.0).round() as u8
	};
	let mut pixels: Vec<u8> = Vec::with_capacity(picture.width * picture.height * 4);
	for row in 0..picture.height {
		for column in 0..picture.width {
			let luma = (picture.plane_list[0][row * picture.width + column] as f32 - bias_y) / range_y;
			let (red, green, blue) = if picture.is_monochrome {
				(luma, luma, luma)
			}
			else {
				let raw_chroma_blue = read_upsampled_chroma(picture, &picture.plane_list[1], column, row);
				let raw_chroma_red = read_upsampled_chroma(picture, &picture.plane_list[2], column, row);
				if is_identity {
					// Identity(GBR) 는 크로마도 Y 의 범위로 읽는다.
					(((raw_chroma_red - bias_y) / range_y), luma, ((raw_chroma_blue - bias_y) / range_y))
				}
				else {
					let chroma_blue = (raw_chroma_blue - bias_uv) / range_uv;
					let chroma_red = (raw_chroma_red - bias_uv) / range_uv;
					if is_ycgco {
						let base = luma - chroma_blue;
						(base + chroma_red, luma + chroma_blue, base - chroma_red)
					}
					else {
						let red = luma + (2.0 * (1.0 - luma_red)) * chroma_red;
						let blue = luma + (2.0 * (1.0 - luma_blue)) * chroma_blue;
						let green = luma - (2.0 * ((luma_red * (1.0 - luma_red) * chroma_red) + (luma_blue * (1.0 - luma_blue) * chroma_blue))) / luma_green;
						(red, green, blue)
					}
				}
			};
			let alpha = match alpha_picture {
				Some(alpha_picture) => {
					to_byte(alpha_picture.plane_list[0][row * alpha_picture.width + column] as f32 / alpha_maximum_value)
				}
				None => {
					255
				}
			};
			pixels.push(to_byte(red));
			pixels.push(to_byte(green));
			pixels.push(to_byte(blue));
			pixels.push(alpha);
		}
	}
	pixels
}

#[cfg(test)]
mod tests {
	use super::*;

	// 맥 ImageIO 가 쓴 16 x 16 AVIF. (위 반은 불투명, 아래 반은 알파 128, 왼쪽 반 빨강, 오른쪽 반 파랑, 알파 보조 형 urn:mpeg:hevc:2015:auxid:1)
	const TINY_ALPHA_AVIF_HEX: &str = concat!(
		"000000206674797061766966000000004d695072617669666d6961666d696631000001b76d657461000000000000002168646c7200000000000000007069637400000000000000000000000000000000",
		"2464696e660000001c6472656600000000000000010000000c75726c20000000010000000e7069746d0000000000010000003869696e6600000000000200000015696e66650200000000010000617630",
		"310000000015696e6665020000010002000061763031000000001a69726566000000000000000e6175786c000200010001000000da69707270000000b16970636f00000013636f6c726e636c78000200",
		"020006800000000c636c6c6900cb004000000014697370650000000000000010000000100000000969726f7400000000107069786900000000030808080000000e706978690000000001080000003761",
		"7578430000000075726e3a6d7065673a686576633a323031353a61757869643a31000000000c000000084e01a5040001fe400000000c6176314381000c000000000c6176314381001c00000000216970",
		"6d61000000000000000200010681020305888400020503068789840000002c696c6f630000000044000002000100000001000001e7000000350002000000010000021c00000022000000016d64617400",
		"0000000000006712000a0d000000019ff9fff0204040d08032221400648002082088bfce7af0677d33e22d10f7ec60798f5d5dd235890468da13a2a812000a09000000019ff9fff05432131400638008",
		"2200bfdc0c9de724cc897ef4a3c4",
	);
	// 맥 ImageIO 가 쓴 1100 x 64 AVIF. (512 너비 타일 3 개의 grid, 왼쪽 512 는 빨강, 나머지는 검정)
	const GRID_1100X64_AVIF_HEX: &str = concat!(
		"000000206674797061766966000000004d695072617669666d6961666d696631000001e86d657461000000000000002168646c7200000000000000007069637400000000000000000000000000000000",
		"2464696e660000001c6472656600000000000000010000000c75726c20000000010000000e7069746d0000000000040000006269696e6600000000000400000015696e66650200000100010000617630",
		"310000000015696e66650200000100020000617630310000000015696e66650200000100030000617630310000000015696e6665020000000004000067726964000000001e6972656600000000000000",
		"1264696d6700040003000100020003000000a969707270000000746970636f00000013636f6c726e636c78000200020006800000000c636c6c6900cb0040000000146973706500000000000002000000",
		"02000000001469737065000000000000044c000000400000000969726f7400000000107069786900000000030808080000000c6176314381010c000000002d69706d6100000000000000040001048381",
		"028700020483810287000304838102870004058102040685000000106964617400000002044c004000000050696c6f630100000044000004000100000000000100000218000000620002000000000001",
		"0000027a000000440003000000000001000002be0000004400040001000000010000000000000008000000016d64617400000000000000fa12000a0e0000000c47fffefffc0810103420324e10018480",
		"02082088f4b994a73de0238a59755ae1db1c30e8a235bb7ab20bfea46b7594a596ab50ee863d36f1dcca4806059d80701f3f55c50a4e443606c4e5f27b9f936bd57fd97d1f43db62b9d612000a0e0000",
		"000c47fffefffc081010342032301001848002082088f4b994a31843dc984dafd95461599d3bb542678be7a813a503e23dda21dd83ce0d3041d217381cf012000a0e0000000c47fffefffc0810103420",
		"32301001848002082088f4b994a31843dc984dafd95461599d3bb542678be7a813a503e23dda21dd83ce0d3041d217381cf0",
	);

	//==============================================================================
	// 16진 글을 바이트로.
	//==============================================================================
	fn decode_hex(hex_text: &str) -> Vec<u8> {
		(0..hex_text.len()).step_by(2).map(|character_index| u8::from_str_radix(&hex_text[character_index..character_index + 2], 16).expect("16진")).collect()
	}

	//==============================================================================
	// 픽셀 하나. (프리멀티플라이드 RGBA)
	//==============================================================================
	fn read_pixel(image: &Image, x: usize, y: usize) -> [u8; 4] {
		let pixels = image.get_pixels();
		let index = (y * image.get_width() as usize + x) * 4;
		[pixels[index], pixels[index + 1], pixels[index + 2], pixels[index + 3]]
	}

	//==============================================================================
	// 두 픽셀이 손실 압축의 오차(2) 안에서 같은지.
	//==============================================================================
	fn assert_pixel_near(actual_pixel: [u8; 4], expected_pixel: [u8; 4]) {
		for channel in 0..4 {
			assert!((actual_pixel[channel] as i32 - expected_pixel[channel] as i32).abs() <= 2, "{:?} 와 {:?}", actual_pixel, expected_pixel);
		}
	}

	//==============================================================================
	// 알파 보조 형이 hevc 꼴(맥)이어도 알파를 읽고, grid 는 타일을 이어 출력 크기로 자른다.
	//==============================================================================
	#[test]
	fn decode_alpha_and_grid() {
		let alpha_bytes = decode_hex(TINY_ALPHA_AVIF_HEX);
		assert_eq!(crate::core::image::sniff_image_mime_type(&alpha_bytes), Some("image/avif"));
		let alpha_image = Image::load_from_memory(&alpha_bytes).expect("알파 AVIF");
		assert_eq!((alpha_image.get_width(), alpha_image.get_height()), (16, 16));
		assert_pixel_near(read_pixel(&alpha_image, 2, 2), [255, 0, 0, 255]);
		assert_pixel_near(read_pixel(&alpha_image, 12, 2), [0, 0, 255, 255]);
		assert_pixel_near(read_pixel(&alpha_image, 2, 12), [128, 0, 0, 128]);
		assert_pixel_near(read_pixel(&alpha_image, 12, 12), [0, 0, 128, 128]);
		let grid_image = Image::load_from_memory(&decode_hex(GRID_1100X64_AVIF_HEX)).expect("grid AVIF");
		assert_eq!((grid_image.get_width(), grid_image.get_height()), (1100, 64));
		assert_pixel_near(read_pixel(&grid_image, 100, 32), [255, 0, 0, 255]);
		assert_pixel_near(read_pixel(&grid_image, 600, 32), [0, 0, 0, 255]);
		assert_pixel_near(read_pixel(&grid_image, 1099, 63), [0, 0, 0, 255]);
		// 잘린 파일은 실패다.
		assert!(Image::load_from_memory(&alpha_bytes[..alpha_bytes.len() / 2]).is_err());
	}

	//==============================================================================
	// clap 은 원점이 (0, 0) 일 때만 자르고, irot 는 반시계, imir 는 axis 0 이 위아래, 1 이 좌우다.
	//==============================================================================
	#[test]
	fn clean_aperture_rotation_and_mirror() {
		// 4 x 2 그림에서 깨끗한 크기 2 x 2 와 자리 -1 은 원점 (0, 0), 자리 0 은 원점 (1, 0) 이라 무시한다.
		assert_eq!(read_clean_aperture_size(4, 2, (2, 1), (2, 1), (-1, 1), (0, 1)), Some((2, 2)));
		assert_eq!(read_clean_aperture_size(4, 2, (2, 1), (2, 1), (0, 1), (0, 1)), None);
		assert_eq!(read_clean_aperture_size(4, 2, (4, 1), (2, 1), (0, 1), (0, 1)), Some((4, 2)));
		assert_eq!(read_clean_aperture_size(4, 2, (3, 2), (2, 1), (0, 1), (0, 1)), None);
		// 픽셀 하나에 번호를 적은 3 x 2 그림. (0 1 2 / 3 4 5)
		let pixels: Vec<u8> = (0..6u8).flat_map(|pixel_number| [pixel_number, 0, 0, 255]).collect();
		let read_numbers = |pixels: &[u8]| -> Vec<u8> {
			pixels.chunks_exact(4).map(|pixel| pixel[0]).collect()
		};
		assert_eq!(read_numbers(&crop_pixels(&pixels, 3, 2, 1)), vec![0, 1]);
		// 반시계 90 도: 오른쪽 칸이 맨 위 줄로 간다. (2 5 / 1 4 / 0 3)
		assert_eq!(read_numbers(&rotate_pixels_anticlockwise(&pixels, 3, 2)), vec![2, 5, 1, 4, 0, 3]);
		let mut top_bottom_pixels = pixels.clone();
		mirror_pixels(&mut top_bottom_pixels, 3, 2, 0);
		assert_eq!(read_numbers(&top_bottom_pixels), vec![3, 4, 5, 0, 1, 2]);
		let mut left_right_pixels = pixels.clone();
		mirror_pixels(&mut left_right_pixels, 3, 2, 1);
		assert_eq!(read_numbers(&left_right_pixels), vec![2, 1, 0, 5, 4, 3]);
	}
}
