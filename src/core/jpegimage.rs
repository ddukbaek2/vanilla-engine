//==============================================================================
//! JPEG 그림 풀기. (core/image.rs 가 부른다, 데스크탑과 웹 판 모두)
//! - 원본은 브라우저가 JPEG 을 푼다. (크롬 158 은 libjpeg-turbo 3.1.0 과 블링크 JPEGImageDecoder) 엔진은 같은 셈을 직접 짠 순수 러스트로 푼다.
//!   셈은 ITU-T T.81(허프만 기준선, 확장 순차, 프로그레시브, 재시작 표시), JFIF 1.02, Adobe APP14(transform) 명세를 따르고,
//!   크롬과 같은 화소를 내려고 libjpeg-turbo 3.1.0 의 정수 상수와 차례(아래)를 원문으로 확인해 따랐다. 코드는 옮기지 않았다. (THIRD_PARTY_LICENSES.md 실린 자료 절)
//! - 크롬이 고르는 설정: 블링크 jpeg_image_decoder.cc 는 dct_method = JDCT_ISLOW, do_fancy_upsampling = TRUE, do_block_smoothing = TRUE 이고
//!   YCbCr, 회색, RGB 는 RGBA 로(JCS_EXT_RGBA), CMYK 와 YCCK 는 CMYK 로 받아 화소마다 R = C x K / 255 (뒤집힌 CMYK 로 본다)로 바꾼다.
//!   WebGL texImage2D 는 ImageExtractor 가 그림을 RGB 로 다시 풀어(YUV 로 푸는 길은 GPU 그리기 쪽만) 이 셈이 WebGL 결과다.
//! - 역 DCT: libjpeg-turbo jidctint.c jpeg_idct_islow 의 셈(CONST_BITS 13, PASS1_BITS 2, FIX_0_298631336 ~ FIX_3_072711026 의 13 비트 상수, 두 번 가르기).
//!   범위 밖 값은 자른다. (크롬이 이 맥과 x86 에서 쓰는 SIMD 판이 포화 좁히기로 자르는 것과 같다)
//! - 크로마 늘리기: libjpeg-turbo jdsample.c 의 고르기와 셈. 가로 두 배(h2v1)는 3:1 에 반올림 +1, +2, 가로세로 두 배(h2v2)는 세로 3:1 합을 가로 3:1 로 섞어 +8, +7 뒤 16 으로 나누고,
//!   세로 두 배(h1v2)는 +1, +2 뒤 4 로 나눈다. 폭이 2 이하면 h2v1, h2v2 는 되풀이, 그 밖의 정수 배는 되풀이. 위 가장자리는 첫 줄, 아래는 마지막 실제 줄, 오른쪽은 마지막 칸을 되쓴다.
//! - 색 바꾸기: libjpeg-turbo jdcolor.c 의 16 비트 표. R = Y + ((FIX(1.402) x (Cr - 128) + 2^15) >> 16), B = Y + ((FIX(1.772) x (Cb - 128) + 2^15) >> 16),
//!   G = Y + ((-FIX(0.34414) x (Cb - 128) - FIX(0.71414) x (Cr - 128) + 2^15) >> 16) 를 0 ~ 255 로 자른다. YCCK 는 같은 셈의 255 에서 뺀 값과 K.
//! - 색 공간 가리기: libjpeg-turbo jdapimin.c default_decompress_parms. 성분 3 개는 JFIF 면 YCbCr, Adobe 면 transform 0 이 RGB 이고 그 밖은 YCbCr,
//!   표시가 없으면 성분 번호 1, 2, 3 이 YCbCr, 'R', 'G', 'B' 가 RGB, 그 밖은 YCbCr. 성분 4 개는 Adobe transform 0 이 CMYK, 그 밖과 표시 없음은 YCCK 와 CMYK.
//! - 양자화 표는 성분이 처음 든 스캔에서 묶는다. (libjpeg-turbo jdinput.c latch_quant_tables 처럼)
//! - 무손실(SOF3, 8 비트): T.81 Annex H 의 예측 1 ~ 7 과 점 변환. libjpeg-turbo 는 무손실에서 손실 있는 색 바꾸기를 하지 않아(jdcolor.c) RGB 와 CMYK 만 풀린다.
//! - 빠르게: 허프만은 11 비트 미리 보기 표와 AC 부호, 크기 비트를 한 번에 푸는 표, 0xFF 가 없는 8 바이트를 한꺼번에 넣는 64 비트 비트 버퍼. 역 DCT 는 32 비트로 넘치지 않는 블록을
//!   8 갈래 한꺼번에(자동 벡터화), 늘리기와 색 바꾸기는 줄마다 묶어 바로 RGBA 로 쓴다(그림 크기의 성분 평면을 따로 두지 않는다). 모두 같은 정수 셈과 같은 비트 차례라 결과는 비트까지 같다.
//! - EXIF 방향(APP1 의 IFD0 태그 0x0112)은 크롬처럼 건다. 색 프로필(APP2 ICC)은 core/colorprofile.rs 가 바꾼다.
//! - 크롬이 크기만 읽고 풀지 못하는 것(12 비트와 16 비트, 산술 부호 SOF9 ~ SOF11, 회색과 YCbCr 무손실)은 그 크기의 투명한 그림이다. 크롬 158 은 onload 뒤 Canvas2D 에 아무것도 그리지 않고
//!   WebGL texImage2D 는 INVALID_VALUE 로 올리지 않는다. (크롬 빌드는 jpeg_read_scanlines 의 8 비트만 쓰고 jdarith.c 를 넣지 않는다) 첫 스캔 앞에서 잘린 파일은 크롬처럼 실패다.
//! - 크롬 158 WebGL texImage2D 와 시험 그림 51 장(크로마 꼴 여섯, 프로그레시브, 재시작, 회색, RGB, CMYK, YCCK, ICC, EXIF 방향, 무손실, 1x1 ~ 33x18)이 비트까지 같다.
//! - 원본(크롬)과 다른 것: 계층 JPEG(SOF5 ~ SOF7, SOF13 ~ SOF15)은 libjpeg 처럼 풀지 않는다(크롬이 어떻게 그리는지는 견주지 않았다).
//!   프로그레시브의 블록 다듬기(do_block_smoothing)는 짜지 않았다. 모든 계수를 다 받은 그림에서는 쓰이지 않는다(libjpeg-turbo jdcoefct.c smoothing_ok). 첫 9 개 AC 계수 가운데 어느 스캔에도 없는 것이 있는 그림에서만 크롬과 다르다.
//!   다 받지 못한 파일(엔트로피 자료가 모자람, EOI 가 없음, 남은 스캔이 없음)은 투명한 그림이다. 크롬 158 WebGL texImage2D 는 INVALID_VALUE 로 올리지 않고,
//!   Canvas2D 는 받은 줄까지(프로그레시브는 다 받은 마지막 스캔까지) 그려 엔진과 다르다.
//!   범위를 넘는 계수(16 비트를 넘는 역양자화)는 크롬 SIMD 판과 다를 수 있다.
//==============================================================================

//==============================================================================
/// 푼 JPEG. (곧은 알파 RGBA, 알파는 늘 255)
//==============================================================================
pub(crate) struct DecodedJpeg {
	pub(crate) width: u32,
	pub(crate) height: u32,
	pub(crate) pixels: Vec<u8>,
	/// EXIF 방향. (1 ~ 8, 없으면 1)
	pub(crate) orientation: u8,
}

const ZIGZAG_TO_NATURAL: [usize; 64] = [
	0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28,
	35, 42, 49, 56, 57, 50, 43, 36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

const FAST_BITS: u32 = 11;

//==============================================================================
/// 허프만 표. (T.81 Annex C 의 코드 만들기, FAST_BITS(11) 비트 미리 보기 표)
//==============================================================================
#[derive(Clone)]
struct HuffmanTable {
	/// (길이 << 8) | 값. 0 이면 FAST_BITS 보다 긴 부호.
	fast_lookup: Box<[u16; 1 << FAST_BITS]>,
	/// AC 부호와 그 뒤 크기 비트가 함께 FAST_BITS 안에 드는 것. (값 << 16) | (런 << 8) | (부호 길이 << 4) | 크기, 0 이면 느린 길.
	/// 크기가 0 인 기호(EOB, ZRL)는 넣지 않는다. 같은 비트를 같은 차례로 읽어 값이 느린 길과 같다.
	fast_ac: Box<[i32; 1 << FAST_BITS]>,
	max_code: [i32; 18],
	value_offset: [i32; 18],
	values: Vec<u8>,
}

impl HuffmanTable {
	//==============================================================================
	/// 만든다.
	//==============================================================================
	fn create(code_counts: &[u8; 16], values: &[u8]) -> Result<HuffmanTable, String> {
		let mut fast_lookup = Box::new([0u16; 1 << FAST_BITS]);
		let mut max_code = [-1i32; 18];
		let mut value_offset = [0i32; 18];
		let mut code = 0i32;
		let mut index = 0usize;
		for length in 1..=16usize {
			let count = code_counts[length - 1] as usize;
			if count > 0 {
				value_offset[length] = index as i32 - code;
				for _ in 0..count {
					if index >= values.len() {
						return Err(String::from("JPEG 허프만 표가 짧다"));
					}
					if length as u32 <= FAST_BITS {
						let shift = FAST_BITS - length as u32;
						let first = (code as usize) << shift;
						for entry in fast_lookup.iter_mut().skip(first).take(1 << shift) {
							*entry = ((length as u16) << 8) | values[index] as u16;
						}
					}
					code += 1;
					index += 1;
				}
				max_code[length] = code - 1;
			}
			if code > (1 << length) {
				return Err(String::from("JPEG 허프만 표가 틀렸다"));
			}
			code <<= 1;
		}
		max_code[17] = i32::MAX;
		let mut fast_ac = Box::new([0i32; 1 << FAST_BITS]);
		for (peek, entry) in fast_ac.iter_mut().enumerate() {
			let lookup = fast_lookup[peek];
			if lookup == 0 {
				continue;
			}
			let code_length = (lookup >> 8) as u32;
			let symbol = lookup as u8;
			let run = (symbol >> 4) as i32;
			let size = (symbol & 15) as u32;
			if size == 0 || code_length + size > FAST_BITS {
				continue;
			}
			let raw = ((peek as u32) >> (FAST_BITS - code_length - size)) & ((1 << size) - 1);
			let value = if raw < (1 << (size - 1)) { raw as i32 - (1 << size) + 1 } else { raw as i32 };
			*entry = (value << 16) | (run << 8) | ((code_length as i32) << 4) | size as i32;
		}
		Ok(HuffmanTable { fast_lookup, fast_ac, max_code, value_offset, values: values.to_vec() })
	}
}

//==============================================================================
/// 엔트로피 자료 비트 읽기. (0xFF 00 은 0xFF, 표시를 만나면 그 뒤는 0 으로 채운다, libjpeg 처럼)
//==============================================================================
struct BitReader<'a> {
	data: &'a [u8],
	position: usize,
	bits: u64,
	bit_count: u32,
	is_marker_reached: bool,
	/// 자료 끝을 넘어 읽었는지. (잘린 파일)
	is_data_exhausted: bool,
}

impl<'a> BitReader<'a> {
	//==============================================================================
	/// 만든다.
	//==============================================================================
	fn create(data: &'a [u8], position: usize) -> BitReader<'a> {
		BitReader { data, position, bits: 0, bit_count: 0, is_marker_reached: false, is_data_exhausted: false }
	}

	//==============================================================================
	/// 비트 읽개의 바이트 칸을 채운다.
	//==============================================================================
	fn fill(&mut self) {
		// 빠른 길: 다음 8 바이트에 0xFF 가 없으면 들어갈 만큼 한꺼번에 넣는다. (한 바이트씩 넣는 아래 길과 같은 비트)
		if !self.is_marker_reached && self.bit_count <= 56 && self.position + 8 <= self.data.len() {
			let word = u64::from_be_bytes(self.data[self.position..self.position + 8].try_into().unwrap_or([0xff; 8]));
			let inverted = !word;
			let has_marker_byte = (inverted.wrapping_sub(0x0101_0101_0101_0101) & !inverted & 0x8080_8080_8080_8080) != 0;
			if !has_marker_byte {
				let byte_count = (56 - self.bit_count) / 8 + 1;
				let chunk = if byte_count == 8 { word } else { word >> (64 - byte_count * 8) };
				self.bits |= chunk << (64 - self.bit_count - byte_count * 8);
				self.bit_count += byte_count * 8;
				self.position += byte_count as usize;
				return;
			}
		}
		while self.bit_count <= 56 {
			let mut byte = 0u32;
			if !self.is_marker_reached && self.position >= self.data.len() {
				self.is_data_exhausted = true;
			}
			if !self.is_marker_reached && self.position < self.data.len() {
				let value = self.data[self.position];
				if value == 0xff {
					let mut next_position = self.position + 1;
					while next_position < self.data.len() && self.data[next_position] == 0xff {
						next_position += 1;
					}
					if next_position < self.data.len() && self.data[next_position] == 0 {
						byte = 0xff;
						self.position = next_position + 1;
					}
					else {
						self.is_marker_reached = true;
					}
				}
				else {
					byte = value as u32;
					self.position += 1;
				}
			}
			self.bits |= (byte as u64) << (56 - self.bit_count);
			self.bit_count += 8;
		}
	}

	//==============================================================================
	/// 비트를 정해진 수만큼 읽는다.
	//==============================================================================
	#[inline]
	fn get_bits(&mut self, count: u32) -> i32 {
		if count == 0 {
			return 0;
		}
		if self.bit_count < count {
			self.fill();
		}
		let value = (self.bits >> (64 - count)) as i32;
		self.bits <<= count;
		self.bit_count -= count;
		value
	}

	//==============================================================================
	/// 비트 하나를 읽는다.
	//==============================================================================
	#[inline]
	fn get_bit(&mut self) -> i32 {
		self.get_bits(1)
	}

	//==============================================================================
	/// 허프만 표로 기호 하나를 풀어 읽는다.
	//==============================================================================
	#[inline]
	fn decode(&mut self, table: &HuffmanTable) -> u8 {
		if self.bit_count < 16 {
			self.fill();
		}
		let entry = table.fast_lookup[(self.bits >> (64 - FAST_BITS)) as usize];
		if entry != 0 {
			let length = (entry >> 8) as u32;
			self.bits <<= length;
			self.bit_count -= length;
			return entry as u8;
		}
		let mut code = 0i32;
		for length in 1..=16usize {
			code = (code << 1) | self.get_bit();
			if code <= table.max_code[length] {
				let index = (code + table.value_offset[length]) as usize;
				return table.values.get(index).copied().unwrap_or(0);
			}
		}
		0
	}

	//==============================================================================
	/// 비트를 읽어 부호를 늘린 값으로 돌려준다.
	//==============================================================================
	#[inline]
	fn receive_extend(&mut self, size: u32) -> i32 {
		if size == 0 {
			return 0;
		}
		let value = self.get_bits(size);
		if value < (1 << (size - 1)) {
			value - (1 << size) + 1
		}
		else {
			value
		}
	}

	/// 재시작 표시(RSTn)를 지나고 비트를 비운다.
	fn restart(&mut self) {
		self.bits = 0;
		self.bit_count = 0;
		self.is_marker_reached = false;
		while self.position + 1 < self.data.len() {
			if self.data[self.position] == 0xff && (0xd0..=0xd7).contains(&self.data[self.position + 1]) {
				self.position += 2;
				return;
			}
			if self.data[self.position] == 0xff && self.data[self.position + 1] != 0 && self.data[self.position + 1] != 0xff {
				return;
			}
			self.position += 1;
		}
	}
}

//==============================================================================
/// 프레임의 성분.
//==============================================================================
struct Component {
	identifier: u8,
	horizontal_sampling: usize,
	vertical_sampling: usize,
	quantization_table_index: usize,
	/// 스캔에서 묶은 양자화 표. (자연 차례)
	quantization_table: Option<[u16; 64]>,
	/// 블록 줄의 블록 수. (MCU 로 채운 수)
	blocks_per_line: usize,
	blocks_per_column: usize,
	/// 실제 자료가 있는 블록 수. (그림 크기에서)
	used_blocks_per_line: usize,
	used_blocks_per_column: usize,
	/// 줄인 성분 크기. (ceil(그림 x 표본 / 최대 표본))
	downsampled_width: usize,
	downsampled_height: usize,
	coefficients: Vec<i16>,
	dc_predictor: i32,
	/// 무손실의 표본. (폭 downsampled_width, MCU 로 채운 줄 수)
	lossless_samples: Vec<u8>,
}

struct ScanComponent {
	component_index: usize,
	dc_table_index: usize,
	ac_table_index: usize,
}

#[derive(Clone, Copy, PartialEq)]
enum ColorTransform {
	YCbCr,
	Rgb,
	Gray,
	Cmyk,
	Ycck,
}

//==============================================================================
/// JPEG 바이트를 RGBA 로 푼다.
///
/// # 돌려줌
/// 곧은 알파 RGBA 와 EXIF 방향. 풀지 못하면 그 까닭을 담은 Err.
//==============================================================================
pub(crate) fn decode_jpeg(bytes: &[u8]) -> Result<DecodedJpeg, String> {
	if bytes.len() < 4 || bytes[0] != 0xff || bytes[1] != 0xd8 {
		return Err(String::from("JPEG 이 아니다"));
	}
	let mut quantization_tables: [Option<[u16; 64]>; 4] = [None; 4];
	let mut dc_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
	let mut ac_tables: [Option<HuffmanTable>; 4] = [None, None, None, None];
	let mut component_list: Vec<Component> = Vec::new();
	let mut width = 0usize;
	let mut height = 0usize;
	let mut is_progressive = false;
	let mut max_horizontal_sampling = 1usize;
	let mut max_vertical_sampling = 1usize;
	let mut restart_interval = 0usize;
	let mut is_jfif = false;
	let mut adobe_transform: Option<u8> = None;
	let mut orientation = 1u8;
	let mut is_frame_read = false;
	let mut is_lossless = false;
	let mut is_unsupported_coding = false;
	let mut is_truncated = false;
	let mut is_end_reached = false;
	let mut scan_count = 0usize;
	let mut position = 2usize;
	loop {
		while position < bytes.len() && bytes[position] != 0xff {
			position += 1;
		}
		while position < bytes.len() && bytes[position] == 0xff {
			position += 1;
		}
		if position >= bytes.len() {
			break;
		}
		let marker = bytes[position];
		position += 1;
		if marker == 0xd9 {
			is_end_reached = true;
			break;
		}
		if marker == 0xd8 || (0xd0..=0xd7).contains(&marker) || marker == 0x01 || marker == 0 {
			continue;
		}
		if position + 2 > bytes.len() {
			break;
		}
		let segment_length = ((bytes[position] as usize) << 8) | bytes[position + 1] as usize;
		if segment_length < 2 {
			return Err(String::from("JPEG 표시 길이가 틀렸다"));
		}
		if position + segment_length > bytes.len() {
			// 잘린 파일. 첫 스캔 앞에서 잘리면 크롬도 머리(jpeg_read_header)를 다 읽지 못해 그림 실패다.
			if scan_count > 0 {
				is_truncated = true;
				break;
			}
			return Err(String::from("JPEG 이 잘렸다"));
		}
		let segment = &bytes[position + 2..position + segment_length];
		position += segment_length;
		match marker {
			0xdb => {
				read_quantization_tables(segment, &mut quantization_tables)?;
			}
			0xc4 => {
				read_huffman_tables(segment, &mut dc_tables, &mut ac_tables)?;
			}
			0xdd => {
				if segment.len() < 2 {
					return Err(String::from("JPEG DRI 가 짧다"));
				}
				restart_interval = ((segment[0] as usize) << 8) | segment[1] as usize;
			}
			0xe0 => {
				if segment.len() >= 14 && &segment[..5] == b"JFIF\0" {
					is_jfif = true;
				}
			}
			0xe1 => {
				if let Some(value) = read_exif_orientation(segment) {
					orientation = value;
				}
			}
			0xee => {
				if segment.len() >= 12 && &segment[..5] == b"Adobe" {
					adobe_transform = Some(segment[11]);
				}
			}
			0xc0..=0xc3 | 0xc9..=0xcb => {
				if is_frame_read {
					return Err(String::from("JPEG 프레임이 둘이다"));
				}
				is_frame_read = true;
				is_progressive = marker == 0xc2 || marker == 0xca;
				is_lossless = marker == 0xc3 || marker == 0xcb;
				if segment.len() < 6 {
					return Err(String::from("JPEG SOF 가 짧다"));
				}
				// 크롬은 8 비트가 아닌 것과 산술 부호를 크기만 읽고 풀지 못해 투명한 그림이 된다. (jpeg_read_scanlines 는 8 비트만, 빌드에 jdarith.c 가 없다)
				is_unsupported_coding = segment[0] != 8 || marker >= 0xc9;
				height = ((segment[1] as usize) << 8) | segment[2] as usize;
				width = ((segment[3] as usize) << 8) | segment[4] as usize;
				let component_count = segment[5] as usize;
				if width == 0 || height == 0 || component_count == 0 || segment.len() < 6 + component_count * 3 {
					return Err(String::from("JPEG SOF 가 틀렸다"));
				}
				for index in 0..component_count {
					let entry = &segment[6 + index * 3..9 + index * 3];
					let horizontal_sampling = (entry[1] >> 4) as usize;
					let vertical_sampling = (entry[1] & 15) as usize;
					if !(1..=4).contains(&horizontal_sampling) || !(1..=4).contains(&vertical_sampling) || entry[2] > 3 {
						return Err(String::from("JPEG 표본 비율이 틀렸다"));
					}
					component_list.push(Component {
						identifier: entry[0],
						horizontal_sampling,
						vertical_sampling,
						quantization_table_index: entry[2] as usize,
						quantization_table: None,
						blocks_per_line: 0,
						blocks_per_column: 0,
						used_blocks_per_line: 0,
						used_blocks_per_column: 0,
						downsampled_width: 0,
						downsampled_height: 0,
						coefficients: Vec::new(),
						dc_predictor: 0,
						lossless_samples: Vec::new(),
					});
				}
				max_horizontal_sampling = component_list.iter().map(|component| component.horizontal_sampling).max().unwrap_or(1);
				max_vertical_sampling = component_list.iter().map(|component| component.vertical_sampling).max().unwrap_or(1);
				let mcu_columns = width.div_ceil(8 * max_horizontal_sampling);
				let mcu_rows = height.div_ceil(8 * max_vertical_sampling);
				let pixel_count = width.checked_mul(height).ok_or_else(|| String::from("JPEG 이 너무 크다"))?;
				if pixel_count > (1usize << 30) {
					return Err(String::from("JPEG 이 너무 크다"));
				}
				for component in component_list.iter_mut() {
					component.downsampled_width = (width * component.horizontal_sampling).div_ceil(max_horizontal_sampling);
					component.downsampled_height = (height * component.vertical_sampling).div_ceil(max_vertical_sampling);
					component.used_blocks_per_line = component.downsampled_width.div_ceil(8);
					component.used_blocks_per_column = component.downsampled_height.div_ceil(8);
					component.blocks_per_line = mcu_columns * component.horizontal_sampling;
					component.blocks_per_column = mcu_rows * component.vertical_sampling;
					if is_unsupported_coding {
						continue;
					}
					if is_lossless {
						let mcu_rows = height.div_ceil(max_vertical_sampling);
						component.lossless_samples = vec![0u8; component.downsampled_width * mcu_rows * component.vertical_sampling];
					}
					else {
						component.coefficients = vec![0i16; component.blocks_per_line * component.blocks_per_column * 64];
					}
				}
				if is_unsupported_coding {
					break;
				}
			}
			0xc5..=0xc7 | 0xcd..=0xcf => {
				return Err(String::from("계층 JPEG 은 풀지 않는다 (libjpeg 처럼)"));
			}
			0xda => {
				if !is_frame_read {
					return Err(String::from("JPEG 프레임 앞에 스캔이 있다"));
				}
				let scan_component_count = *segment.first().unwrap_or(&0) as usize;
				if scan_component_count == 0 || scan_component_count > 4 || segment.len() < 4 + scan_component_count * 2 {
					return Err(String::from("JPEG SOS 가 틀렸다"));
				}
				let mut scan_component_list = Vec::with_capacity(scan_component_count);
				for index in 0..scan_component_count {
					let identifier = segment[1 + index * 2];
					let table_byte = segment[2 + index * 2];
					let component_index = component_list.iter().position(|component| component.identifier == identifier).ok_or_else(|| String::from("JPEG 스캔 성분이 없다"))?;
					scan_component_list.push(ScanComponent { component_index, dc_table_index: (table_byte >> 4) as usize & 3, ac_table_index: (table_byte & 15) as usize & 3 });
				}
				let parameter = &segment[1 + scan_component_count * 2..];
				let spectral_start = parameter[0] as usize;
				let spectral_end = parameter[1] as usize;
				let approximation_high = (parameter[2] >> 4) as u32;
				let approximation_low = (parameter[2] & 15) as u32;
				for scan_component in scan_component_list.iter() {
					let component = &mut component_list[scan_component.component_index];
					if !is_lossless && component.quantization_table.is_none() {
						component.quantization_table = Some(quantization_tables[component.quantization_table_index].ok_or_else(|| String::from("JPEG 양자화 표가 없다"))?);
					}
				}
				let scan = ScanParameter {
					spectral_start,
					spectral_end: spectral_end.min(63),
					approximation_high,
					approximation_low,
					is_progressive,
					restart_interval,
				};
				let (end_position, is_data_exhausted) = if is_lossless {
					decode_lossless_scan(bytes, position, &mut component_list, &scan_component_list, &dc_tables, &scan, width, height, max_horizontal_sampling, max_vertical_sampling)?
				}
				else {
					decode_scan(bytes, position, &mut component_list, &scan_component_list, &dc_tables, &ac_tables, &scan)?
				};
				position = end_position;
				is_truncated |= is_data_exhausted;
				scan_count += 1;
			}
			_ => {}
		}
	}
	// 크롬이 풀지 못하는 꼴은 크기만 알린다. (onload, Canvas2D 는 그리지 않고 WebGL texImage2D 는 INVALID_VALUE 로 올리지 않는다) 엔진은 투명한 그림이다.
	if is_frame_read && is_unsupported_coding {
		return Ok(DecodedJpeg { width: width as u32, height: height as u32, pixels: vec![0u8; width * height * 4], orientation });
	}
	if !is_frame_read || scan_count == 0 {
		return Err(String::from("JPEG 에 그림 자료가 없다"));
	}
	// 다 받지 못한 그림(엔트로피 자료가 모자람, EOI 가 없음, 남은 스캔이 없음)도 크롬 WebGL texImage2D 는 올리지 않는다. (Canvas2D 는 받은 데까지 그린다) 엔진은 투명한 그림이다.
	if is_truncated || !is_end_reached {
		return Ok(DecodedJpeg { width: width as u32, height: height as u32, pixels: vec![0u8; width * height * 4], orientation });
	}
	let color_transform = match component_list.len() {
		1 => ColorTransform::Gray,
		3 => {
			if is_jfif {
				ColorTransform::YCbCr
			}
			else if let Some(transform) = adobe_transform {
				if transform == 0 { ColorTransform::Rgb } else { ColorTransform::YCbCr }
			}
			else if component_list[0].identifier == b'R' && component_list[1].identifier == b'G' && component_list[2].identifier == b'B' {
				ColorTransform::Rgb
			}
			else if is_lossless {
				ColorTransform::Rgb
			}
			else {
				ColorTransform::YCbCr
			}
		}
		4 => {
			if adobe_transform == Some(0) { ColorTransform::Cmyk } else if adobe_transform.is_some() { ColorTransform::Ycck } else { ColorTransform::Cmyk }
		}
		_ => {
			return Err(String::from("성분이 1, 3, 4 개가 아닌 JPEG 은 풀지 않는다 (크롬처럼)"));
		}
	};
	if is_lossless && color_transform != ColorTransform::Rgb && color_transform != ColorTransform::Cmyk {
		// 무손실은 손실 있는 색 바꾸기를 하지 않는다. (jdcolor.c jinit_color_deconverter 가 RGBA 는 RGB 에서만, CMYK 는 CMYK 에서만 받는다) 크롬은 투명한 그림이다.
		return Ok(DecodedJpeg { width: width as u32, height: height as u32, pixels: vec![0u8; width * height * 4], orientation });
	}
	let transformed_list: Vec<Vec<u8>> = if is_lossless { Vec::new() } else { component_list.iter().map(inverse_transform_component).collect() };
	let sources: Vec<SamplePlane> = component_list.iter().enumerate().map(|(index, component)| {
		if is_lossless {
			SamplePlane { component, plane: &component.lossless_samples, stride: component.downsampled_width, is_fancy: false }
		}
		else {
			SamplePlane { component, plane: &transformed_list[index], stride: component.used_blocks_per_line * 8, is_fancy: true }
		}
	}).collect();
	let pixels = convert_rows(&sources, width, height, max_horizontal_sampling, max_vertical_sampling, color_transform);
	Ok(DecodedJpeg { width: width as u32, height: height as u32, pixels, orientation })
}

//==============================================================================
/// 양자화 표 구간을 읽는다.
//==============================================================================
fn read_quantization_tables(segment: &[u8], quantization_tables: &mut [Option<[u16; 64]>; 4]) -> Result<(), String> {
	let mut position = 0usize;
	while position < segment.len() {
		let precision = segment[position] >> 4;
		let index = (segment[position] & 15) as usize;
		position += 1;
		if index > 3 {
			return Err(String::from("JPEG 양자화 표 번호가 틀렸다"));
		}
		let mut table = [0u16; 64];
		for zigzag_index in 0..64 {
			let value = if precision == 0 {
				let value = *segment.get(position).ok_or_else(|| String::from("JPEG DQT 가 짧다"))? as u16;
				position += 1;
				value
			}
			else {
				if position + 2 > segment.len() {
					return Err(String::from("JPEG DQT 가 짧다"));
				}
				let value = ((segment[position] as u16) << 8) | segment[position + 1] as u16;
				position += 2;
				value
			};
			table[ZIGZAG_TO_NATURAL[zigzag_index]] = value;
		}
		quantization_tables[index] = Some(table);
	}
	Ok(())
}

//==============================================================================
/// 허프만 표 구간을 읽는다.
//==============================================================================
fn read_huffman_tables(segment: &[u8], dc_tables: &mut [Option<HuffmanTable>; 4], ac_tables: &mut [Option<HuffmanTable>; 4]) -> Result<(), String> {
	let mut position = 0usize;
	while position < segment.len() {
		if position + 17 > segment.len() {
			return Err(String::from("JPEG DHT 가 짧다"));
		}
		let class = segment[position] >> 4;
		let index = (segment[position] & 15) as usize;
		if index > 3 || class > 1 {
			return Err(String::from("JPEG 허프만 표 번호가 틀렸다"));
		}
		let mut code_counts = [0u8; 16];
		code_counts.copy_from_slice(&segment[position + 1..position + 17]);
		let value_count: usize = code_counts.iter().map(|count| *count as usize).sum();
		position += 17;
		if value_count > 256 || position + value_count > segment.len() {
			return Err(String::from("JPEG DHT 가 짧다"));
		}
		let table = HuffmanTable::create(&code_counts, &segment[position..position + value_count])?;
		position += value_count;
		if class == 0 {
			dc_tables[index] = Some(table);
		}
		else {
			ac_tables[index] = Some(table);
		}
	}
	Ok(())
}

//==============================================================================
/// EXIF 방향 읽기. (APP1 "Exif\0\0" 뒤 TIFF 머리의 IFD0 에서 태그 0x0112)
//==============================================================================
fn read_exif_orientation(segment: &[u8]) -> Option<u8> {
	if segment.len() < 14 || &segment[..6] != b"Exif\0\0" {
		return None;
	}
	let tiff = &segment[6..];
	let is_little_endian = match &tiff[..2] {
		b"II" => true,
		b"MM" => false,
		_ => return None,
	};
	let read_u16 = |offset: usize| -> Option<u16> {
		let pair = tiff.get(offset..offset + 2)?;
		Some(if is_little_endian { u16::from_le_bytes([pair[0], pair[1]]) } else { u16::from_be_bytes([pair[0], pair[1]]) })
	};
	let read_u32 = |offset: usize| -> Option<u32> {
		let quad = tiff.get(offset..offset + 4)?;
		Some(if is_little_endian { u32::from_le_bytes([quad[0], quad[1], quad[2], quad[3]]) } else { u32::from_be_bytes([quad[0], quad[1], quad[2], quad[3]]) })
	};
	if read_u16(2)? != 42 {
		return None;
	}
	let directory_offset = read_u32(4)? as usize;
	let entry_count = read_u16(directory_offset)? as usize;
	for index in 0..entry_count {
		let entry_offset = directory_offset + 2 + index * 12;
		if read_u16(entry_offset)? == 0x0112 {
			let value_type = read_u16(entry_offset + 2)?;
			if value_type != 3 {
				return None;
			}
			let value = read_u16(entry_offset + 8)?;
			if (1..=8).contains(&value) {
				return Some(value as u8);
			}
			return None;
		}
	}
	None
}

struct ScanParameter {
	spectral_start: usize,
	spectral_end: usize,
	approximation_high: u32,
	approximation_low: u32,
	is_progressive: bool,
	restart_interval: usize,
}

//==============================================================================
/// 스캔 하나 풀기. (T.81 F.2 순차, G.1.2 프로그레시브)
///
/// # 돌려줌
/// 스캔 자료 뒤의 자리. (다음 표시를 찾을 곳)
//==============================================================================
fn decode_scan(bytes: &[u8], position: usize, component_list: &mut [Component], scan_component_list: &[ScanComponent], dc_tables: &[Option<HuffmanTable>; 4], ac_tables: &[Option<HuffmanTable>; 4], scan: &ScanParameter) -> Result<(usize, bool), String> {
	let mut reader = BitReader::create(bytes, position);
	for scan_component in scan_component_list.iter() {
		component_list[scan_component.component_index].dc_predictor = 0;
	}
	let is_dc_scan = scan.spectral_start == 0;
	let needs_dc_table = !scan.is_progressive || (is_dc_scan && scan.approximation_high == 0);
	let needs_ac_table = !scan.is_progressive || !is_dc_scan;
	for scan_component in scan_component_list.iter() {
		if needs_dc_table && dc_tables[scan_component.dc_table_index].is_none() {
			return Err(String::from("JPEG DC 허프만 표가 없다"));
		}
		if needs_ac_table && ac_tables[scan_component.ac_table_index].is_none() {
			return Err(String::from("JPEG AC 허프만 표가 없다"));
		}
	}
	let empty_table = HuffmanTable { fast_lookup: Box::new([0; 1 << FAST_BITS]), fast_ac: Box::new([0; 1 << FAST_BITS]), max_code: [-1; 18], value_offset: [0; 18], values: Vec::new() };
	let mut end_of_band_run = 0u32;
	let decode_block = |reader: &mut BitReader, component: &mut Component, block_index: usize, dc_table: &HuffmanTable, ac_table: &HuffmanTable, end_of_band_run: &mut u32| {
		let block: &mut [i16; 64] = (&mut component.coefficients[block_index * 64..block_index * 64 + 64]).try_into().expect("블록은 64 개");
		if !scan.is_progressive {
			let size = reader.decode(dc_table) as u32;
			let difference = reader.receive_extend(size.min(16));
			component.dc_predictor = component.dc_predictor.wrapping_add(difference);
			block[0] = component.dc_predictor as i16;
			let mut index = 1usize;
			while index < 64 {
				if reader.bit_count < 16 {
					reader.fill();
				}
				let fast = ac_table.fast_ac[(reader.bits >> (64 - FAST_BITS)) as usize];
				if fast != 0 {
					let code_length = ((fast >> 4) & 15) as u32;
					index += ((fast >> 8) & 255) as usize;
					if index > 63 {
						// 느린 길처럼 부호만 읽고 크기 비트는 읽지 않는다
						reader.bits <<= code_length;
						reader.bit_count -= code_length;
						break;
					}
					let length = code_length + (fast & 15) as u32;
					reader.bits <<= length;
					reader.bit_count -= length;
					block[ZIGZAG_TO_NATURAL[index] & 63] = (fast >> 16) as i16;
					index += 1;
					continue;
				}
				let symbol = reader.decode(ac_table);
				let run = (symbol >> 4) as usize;
				let size = (symbol & 15) as u32;
				if size == 0 {
					if run != 15 {
						break;
					}
					index += 16;
					continue;
				}
				index += run;
				if index > 63 {
					break;
				}
				block[ZIGZAG_TO_NATURAL[index]] = reader.receive_extend(size) as i16;
				index += 1;
			}
			return;
		}
		if is_dc_scan {
			if scan.approximation_high == 0 {
				let size = reader.decode(dc_table) as u32;
				let difference = reader.receive_extend(size.min(16));
				component.dc_predictor = component.dc_predictor.wrapping_add(difference);
				block[0] = (component.dc_predictor << scan.approximation_low) as i16;
			}
			else if reader.get_bit() != 0 {
				block[0] |= (1i32 << scan.approximation_low) as i16;
			}
			return;
		}
		if scan.approximation_high == 0 {
			if *end_of_band_run > 0 {
				*end_of_band_run -= 1;
				return;
			}
			let mut index = scan.spectral_start;
			while index <= scan.spectral_end {
				let symbol = reader.decode(ac_table);
				let run = (symbol >> 4) as u32;
				let size = (symbol & 15) as u32;
				if size == 0 {
					if run < 15 {
						*end_of_band_run = (1u32 << run) - 1;
						if run > 0 {
							*end_of_band_run += reader.get_bits(run) as u32;
						}
						break;
					}
					index += 16;
					continue;
				}
				index += run as usize;
				if index > 63 {
					break;
				}
				block[ZIGZAG_TO_NATURAL[index]] = (reader.receive_extend(size) * (1 << scan.approximation_low)) as i16;
				index += 1;
			}
			return;
		}
		let positive_bit = 1i16 << scan.approximation_low;
		let negative_bit = (-1i16) << scan.approximation_low;
		let mut index = scan.spectral_start;
		if *end_of_band_run == 0 {
			while index <= scan.spectral_end {
				let symbol = reader.decode(ac_table);
				let mut run = (symbol >> 4) as i32;
				let size = symbol & 15;
				let mut value = 0i16;
				if size != 0 {
					value = if reader.get_bit() != 0 { positive_bit } else { negative_bit };
				}
				else if run != 15 {
					*end_of_band_run = 1u32 << run;
					if run > 0 {
						*end_of_band_run += reader.get_bits(run as u32) as u32;
					}
					break;
				}
				while index <= scan.spectral_end {
					let coefficient = &mut block[ZIGZAG_TO_NATURAL[index]];
					if *coefficient != 0 {
						if reader.get_bit() != 0 && (*coefficient & positive_bit) == 0 {
							if *coefficient >= 0 {
								*coefficient = coefficient.wrapping_add(positive_bit);
							}
							else {
								*coefficient = coefficient.wrapping_add(negative_bit);
							}
						}
					}
					else {
						run -= 1;
						if run < 0 {
							break;
						}
					}
					index += 1;
				}
				if value != 0 && index <= scan.spectral_end {
					block[ZIGZAG_TO_NATURAL[index]] = value;
				}
				index += 1;
			}
		}
		if *end_of_band_run > 0 {
			while index <= scan.spectral_end {
				let coefficient = &mut block[ZIGZAG_TO_NATURAL[index]];
				if *coefficient != 0 && reader.get_bit() != 0 && (*coefficient & positive_bit) == 0 {
					if *coefficient >= 0 {
						*coefficient = coefficient.wrapping_add(positive_bit);
					}
					else {
						*coefficient = coefficient.wrapping_add(negative_bit);
					}
				}
				index += 1;
			}
			*end_of_band_run -= 1;
		}
	};
	let mut restart_countdown = scan.restart_interval;
	let mut handle_restart = |reader: &mut BitReader, component_list: &mut [Component], end_of_band_run: &mut u32| {
		if scan.restart_interval == 0 {
			return;
		}
		if restart_countdown == 0 {
			reader.restart();
			for scan_component in scan_component_list.iter() {
				component_list[scan_component.component_index].dc_predictor = 0;
			}
			*end_of_band_run = 0;
			restart_countdown = scan.restart_interval;
		}
		restart_countdown -= 1;
	};
	if scan_component_list.len() == 1 {
		let scan_component = &scan_component_list[0];
		let dc_table = dc_tables[scan_component.dc_table_index].as_ref().unwrap_or(&empty_table);
		let ac_table = ac_tables[scan_component.ac_table_index].as_ref().unwrap_or(&empty_table);
		let (used_columns, used_rows, blocks_per_line) = {
			let component = &component_list[scan_component.component_index];
			(component.used_blocks_per_line, component.used_blocks_per_column, component.blocks_per_line)
		};
		for block_row in 0..used_rows {
			for block_column in 0..used_columns {
				handle_restart(&mut reader, component_list, &mut end_of_band_run);
				let component = &mut component_list[scan_component.component_index];
				decode_block(&mut reader, component, block_row * blocks_per_line + block_column, dc_table, ac_table, &mut end_of_band_run);
			}
		}
	}
	else {
		let width_in_mcu = component_list[0].blocks_per_line / component_list[0].horizontal_sampling;
		let height_in_mcu = component_list[0].blocks_per_column / component_list[0].vertical_sampling;
		for mcu_row in 0..height_in_mcu {
			for mcu_column in 0..width_in_mcu {
				handle_restart(&mut reader, component_list, &mut end_of_band_run);
				for scan_component in scan_component_list.iter() {
					let dc_table = dc_tables[scan_component.dc_table_index].as_ref().unwrap_or(&empty_table);
					let ac_table = ac_tables[scan_component.ac_table_index].as_ref().unwrap_or(&empty_table);
					let component = &mut component_list[scan_component.component_index];
					let horizontal_sampling = component.horizontal_sampling;
					let vertical_sampling = component.vertical_sampling;
					let blocks_per_line = component.blocks_per_line;
					for block_y in 0..vertical_sampling {
						for block_x in 0..horizontal_sampling {
							let block_row = mcu_row * vertical_sampling + block_y;
							let block_column = mcu_column * horizontal_sampling + block_x;
							decode_block(&mut reader, component, block_row * blocks_per_line + block_column, dc_table, ac_table, &mut end_of_band_run);
						}
					}
				}
			}
		}
	}
	finish_scan(bytes, &reader)
}

//==============================================================================
/// 무손실 스캔 하나 풀기. (T.81 Annex H, libjpeg-turbo jdlhuff.c, jddiffct.c, jdlossls.c 의 차례)
/// - 차이: 크기 16 은 늘 32768, 그 밖은 receive_extend. 값은 (차이 + 예측) & 0xFFFF 이고 내보낼 때 << Pt 를 8 비트로.
/// - 예측: 스캔과 재시작 구간의 첫 줄은 첫 표본이 2^(P - Pt - 1), 나머지는 Ra. 다른 줄의 첫 표본은 Rb, 나머지는 Ss 의 예측(1 Ra, 2 Rb, 3 Rc, 4 Ra + Rb - Rc,
///   5 Ra + ((Rb - Rc) >> 1), 6 Rb + ((Ra - Rc) >> 1), 7 (Ra + Rb) >> 1). 재시작 간격은 MCU 줄의 배수다.
//==============================================================================
#[allow(clippy::too_many_arguments)]
fn decode_lossless_scan(bytes: &[u8], position: usize, component_list: &mut [Component], scan_component_list: &[ScanComponent], dc_tables: &[Option<HuffmanTable>; 4], scan: &ScanParameter, width: usize, height: usize, max_horizontal_sampling: usize, max_vertical_sampling: usize) -> Result<(usize, bool), String> {
	let predictor = scan.spectral_start;
	let point_transform = scan.approximation_low;
	if !(1..=7).contains(&predictor) || point_transform >= 8 {
		return Err(String::from("JPEG 무손실 스캔 값이 틀렸다"));
	}
	for scan_component in scan_component_list.iter() {
		if dc_tables[scan_component.dc_table_index].is_none() {
			return Err(String::from("JPEG 허프만 표가 없다"));
		}
	}
	let is_interleaved = scan_component_list.len() > 1;
	let (mcu_columns, mcu_rows) = if is_interleaved {
		(width.div_ceil(max_horizontal_sampling), height.div_ceil(max_vertical_sampling))
	}
	else {
		let component = &component_list[scan_component_list[0].component_index];
		(component.downsampled_width, component.downsampled_height)
	};
	if scan.restart_interval > 0 && scan.restart_interval % mcu_columns != 0 {
		return Err(String::from("JPEG 무손실 재시작 간격이 MCU 줄의 배수가 아니다"));
	}
	let restart_rows = if scan.restart_interval > 0 { scan.restart_interval / mcu_columns } else { usize::MAX };
	let mut reader = BitReader::create(bytes, position);
	// 성분마다 차이 버퍼. (MCU 줄 하나, 폭은 MCU 로 채운 표본 수)
	let mut difference_list: Vec<Vec<i32>> = scan_component_list.iter().map(|scan_component| {
		let component = &component_list[scan_component.component_index];
		let (block_width, block_height) = if is_interleaved { (component.horizontal_sampling, component.vertical_sampling) } else { (1, 1) };
		vec![0i32; mcu_columns * block_width * block_height]
	}).collect();
	let mut previous_row_list: Vec<Vec<i32>> = scan_component_list.iter().map(|scan_component| vec![0i32; component_list[scan_component.component_index].downsampled_width]).collect();
	let mut current_row = Vec::new();
	let initial_predictor = 1i32 << (8 - point_transform - 1);
	let mut rows_to_restart = restart_rows;
	for mcu_row in 0..mcu_rows {
		let mut is_first_row = mcu_row == 0;
		if scan.restart_interval > 0 && rows_to_restart == 0 {
			reader.restart();
			rows_to_restart = restart_rows;
			is_first_row = true;
		}
		rows_to_restart = rows_to_restart.wrapping_sub(1);
		for mcu_column in 0..mcu_columns {
			for (scan_index, scan_component) in scan_component_list.iter().enumerate() {
				let component = &component_list[scan_component.component_index];
				let (block_width, block_height) = if is_interleaved { (component.horizontal_sampling, component.vertical_sampling) } else { (1, 1) };
				let table = dc_tables[scan_component.dc_table_index].as_ref().unwrap();
				let row_width = mcu_columns * block_width;
				for block_y in 0..block_height {
					for block_x in 0..block_width {
						let size = reader.decode(table) as u32;
						let difference = if size == 16 { 32768 } else { reader.receive_extend(size.min(15)) };
						difference_list[scan_index][block_y * row_width + mcu_column * block_width + block_x] = difference;
					}
				}
			}
		}
		for (scan_index, scan_component) in scan_component_list.iter().enumerate() {
			let component = &mut component_list[scan_component.component_index];
			let block_height = if is_interleaved { component.vertical_sampling } else { 1 };
			let row_width = mcu_columns * if is_interleaved { component.horizontal_sampling } else { 1 };
			let sample_width = component.downsampled_width;
			for block_y in 0..block_height {
				let difference_row = &difference_list[scan_index][block_y * row_width..block_y * row_width + sample_width];
				let previous_row = &previous_row_list[scan_index];
				current_row.clear();
				let mut left = 0i32;
				for (column, difference) in difference_row.iter().enumerate() {
					let prediction = if is_first_row && block_y == 0 {
						if column == 0 { initial_predictor } else { left }
					}
					else if column == 0 {
						previous_row[0]
					}
					else {
						let above = previous_row[column];
						let above_left = previous_row[column - 1];
						match predictor {
							1 => left,
							2 => above,
							3 => above_left,
							4 => left + above - above_left,
							5 => left + ((above - above_left) >> 1),
							6 => above + ((left - above_left) >> 1),
							_ => (left + above) >> 1,
						}
					};
					left = (difference + prediction) & 0xffff;
					current_row.push(left);
				}
				let sample_row = mcu_row * block_height + block_y;
				let output_offset = sample_row * sample_width;
				if output_offset + sample_width <= component.lossless_samples.len() {
					for (output, value) in component.lossless_samples[output_offset..output_offset + sample_width].iter_mut().zip(current_row.iter()) {
						*output = (*value << point_transform) as u8;
					}
				}
				previous_row_list[scan_index].copy_from_slice(&current_row);
			}
		}
	}
	finish_scan(bytes, &reader)
}

//==============================================================================
/// 스캔 뒤 다음 표시 자리와 자료가 모자랐는지.
//==============================================================================
fn finish_scan(bytes: &[u8], reader: &BitReader) -> Result<(usize, bool), String> {
	let mut end_position = reader.position;
	while end_position + 1 < bytes.len() && !(bytes[end_position] == 0xff && bytes[end_position + 1] != 0 && !(0xd0..=0xd7).contains(&bytes[end_position + 1])) {
		end_position += 1;
	}
	Ok((end_position, reader.is_data_exhausted))
}

const CONST_BITS: u32 = 13;
const PASS1_BITS: u32 = 2;
const FIX_0_298631336: i64 = 2446;
const FIX_0_390180644: i64 = 3196;
const FIX_0_541196100: i64 = 4433;
const FIX_0_765366865: i64 = 6270;
const FIX_0_899976223: i64 = 7373;
const FIX_1_175875602: i64 = 9633;
const FIX_1_501321110: i64 = 12299;
const FIX_1_847759065: i64 = 15137;
const FIX_1_961570560: i64 = 16069;
const FIX_2_053119869: i64 = 16819;
const FIX_2_562915447: i64 = 20995;
const FIX_3_072711026: i64 = 25172;

//==============================================================================
/// 고정소수 곱셈 결과의 자릿수를 되돌린다.
//==============================================================================
#[inline]
fn descale(value: i64, shift: u32) -> i64 {
	(value + (1i64 << (shift - 1))) >> shift
}

/// 32 비트 셈이 넘치지 않는 입력 크기. (두 번의 1 차원 셈 모두 가장 큰 합이 178219 x 입력 + 반올림이라 12049 아래면 2^31 안에 든다)
const NARROW_INPUT_LIMIT: i32 = 12000;

//==============================================================================
/// 정수 역 DCT 한 블록. (jpeg_idct_islow 의 셈, 범위 밖은 자른다)
//==============================================================================
/// - AC 가 모두 0 이면 한 값으로 채운다. (아래 셈에서 열과 줄의 빠른 길이 내는 값과 같다)
/// - 역양자화한 계수와 첫 번째 셈의 결과가 NARROW_INPUT_LIMIT 안이면 32 비트로 8 열(8 줄)을 한꺼번에 셈한다(자동 벡터화). 같은 정수 셈이라 64 비트 셈과 비트까지 같다. 밖이면 64 비트 셈.
fn inverse_transform_block(block: &[i16], quantization_table: &[u16; 64], output: &mut [u8], output_offset: usize, output_stride: usize) {
	let block: &[i16; 64] = block.try_into().expect("블록은 64 개");
	if block[1..].iter().fold(0i16, |accumulated, value| accumulated | *value) == 0 {
		let dc_value = (block[0] as i64 * quantization_table[0] as i64) << PASS1_BITS;
		let sample = (descale(dc_value, PASS1_BITS + 3) + 128).clamp(0, 255) as u8;
		for row in 0..8 {
			output[output_offset + row * output_stride..output_offset + row * output_stride + 8].fill(sample);
		}
		return;
	}
	let mut input = [[0i32; 8]; 8];
	let mut largest = 0i32;
	for row in 0..8 {
		for column in 0..8 {
			let value = block[row * 8 + column] as i32 * quantization_table[row * 8 + column] as i32;
			input[row][column] = value;
			largest = largest.max(value.wrapping_abs());
		}
	}
	// i16 x u16 는 i32 안이다. wrapping_abs 는 i32::MIN 에서만 음수라 아래 비교가 64 비트 길로 보낸다
	if !(0..=NARROW_INPUT_LIMIT).contains(&largest) {
		inverse_transform_block_wide(block, quantization_table, output, output_offset, output_stride);
		return;
	}
	let workspace = inverse_transform_lanes(&input, CONST_BITS - PASS1_BITS);
	let mut transposed = [[0i32; 8]; 8];
	let mut largest = 0i32;
	for row in 0..8 {
		for column in 0..8 {
			transposed[column][row] = workspace[row][column];
			largest = largest.max(workspace[row][column].abs());
		}
	}
	if largest > NARROW_INPUT_LIMIT {
		inverse_transform_block_wide(block, quantization_table, output, output_offset, output_stride);
		return;
	}
	let samples = inverse_transform_lanes(&transposed, CONST_BITS + PASS1_BITS + 3);
	for row in 0..8 {
		let output_row = &mut output[output_offset + row * output_stride..output_offset + row * output_stride + 8];
		for (column, sample) in output_row.iter_mut().enumerate() {
			*sample = (samples[column][row] + 128).clamp(0, 255) as u8;
		}
	}
}

//==============================================================================
/// 1 차원 역 DCT 를 8 갈래에 한꺼번에. (input[k][갈래] 가 k 번째 계수, 돌려준 [k][갈래] 가 k 번째 표본, shift 로 반올림해 나눈다)
//==============================================================================
#[inline(always)]
fn inverse_transform_lanes(input: &[[i32; 8]; 8], shift: u32) -> [[i32; 8]; 8] {
	let mut output = [[0i32; 8]; 8];
	let round = 1i32 << (shift - 1);
	for lane in 0..8 {
		let z2 = input[2][lane];
		let z3 = input[6][lane];
		let z1 = (z2 + z3) * FIX_0_541196100 as i32;
		let tmp2 = z1 + z3 * -(FIX_1_847759065 as i32);
		let tmp3 = z1 + z2 * FIX_0_765366865 as i32;
		let tmp0 = (input[0][lane] + input[4][lane]) << CONST_BITS;
		let tmp1 = (input[0][lane] - input[4][lane]) << CONST_BITS;
		let tmp10 = tmp0 + tmp3;
		let tmp13 = tmp0 - tmp3;
		let tmp11 = tmp1 + tmp2;
		let tmp12 = tmp1 - tmp2;
		let y7 = input[7][lane];
		let y5 = input[5][lane];
		let y3 = input[3][lane];
		let y1 = input[1][lane];
		let z1 = y7 + y1;
		let z2 = y5 + y3;
		let z3 = y7 + y3;
		let z4 = y5 + y1;
		let z5 = (z3 + z4) * FIX_1_175875602 as i32;
		let odd_tmp0 = y7 * FIX_0_298631336 as i32;
		let odd_tmp1 = y5 * FIX_2_053119869 as i32;
		let odd_tmp2 = y3 * FIX_3_072711026 as i32;
		let odd_tmp3 = y1 * FIX_1_501321110 as i32;
		let z1 = z1 * -(FIX_0_899976223 as i32);
		let z2 = z2 * -(FIX_2_562915447 as i32);
		let z3 = z3 * -(FIX_1_961570560 as i32) + z5;
		let z4 = z4 * -(FIX_0_390180644 as i32) + z5;
		let odd0 = odd_tmp0 + z1 + z3;
		let odd1 = odd_tmp1 + z2 + z4;
		let odd2 = odd_tmp2 + z2 + z3;
		let odd3 = odd_tmp3 + z1 + z4;
		output[0][lane] = (tmp10 + odd3 + round) >> shift;
		output[7][lane] = (tmp10 - odd3 + round) >> shift;
		output[1][lane] = (tmp11 + odd2 + round) >> shift;
		output[6][lane] = (tmp11 - odd2 + round) >> shift;
		output[2][lane] = (tmp12 + odd1 + round) >> shift;
		output[5][lane] = (tmp12 - odd1 + round) >> shift;
		output[3][lane] = (tmp13 + odd0 + round) >> shift;
		output[4][lane] = (tmp13 - odd0 + round) >> shift;
	}
	output
}

//==============================================================================
/// 정수 역 DCT 한 블록의 64 비트 셈. (큰 계수의 길)
//==============================================================================
fn inverse_transform_block_wide(block: &[i16], quantization_table: &[u16; 64], output: &mut [u8], output_offset: usize, output_stride: usize) {
	let mut workspace = [0i64; 64];
	for column in 0..8 {
		let coefficient = |row: usize| -> i64 { block[row * 8 + column] as i64 * quantization_table[row * 8 + column] as i64 };
		if (1..8).all(|row| block[row * 8 + column] == 0) {
			let dc_value = coefficient(0) << PASS1_BITS;
			for row in 0..8 {
				workspace[row * 8 + column] = dc_value;
			}
			continue;
		}
		let z2 = coefficient(2);
		let z3 = coefficient(6);
		let z1 = (z2 + z3) * FIX_0_541196100;
		let tmp2 = z1 + z3 * -FIX_1_847759065;
		let tmp3 = z1 + z2 * FIX_0_765366865;
		let z2 = coefficient(0);
		let z3 = coefficient(4);
		let tmp0 = (z2 + z3) << CONST_BITS;
		let tmp1 = (z2 - z3) << CONST_BITS;
		let tmp10 = tmp0 + tmp3;
		let tmp13 = tmp0 - tmp3;
		let tmp11 = tmp1 + tmp2;
		let tmp12 = tmp1 - tmp2;
		let (odd0, odd1, odd2, odd3) = inverse_transform_odd(coefficient(7), coefficient(5), coefficient(3), coefficient(1));
		let shift = CONST_BITS - PASS1_BITS;
		workspace[column] = descale(tmp10 + odd3, shift);
		workspace[7 * 8 + column] = descale(tmp10 - odd3, shift);
		workspace[8 + column] = descale(tmp11 + odd2, shift);
		workspace[6 * 8 + column] = descale(tmp11 - odd2, shift);
		workspace[2 * 8 + column] = descale(tmp12 + odd1, shift);
		workspace[5 * 8 + column] = descale(tmp12 - odd1, shift);
		workspace[3 * 8 + column] = descale(tmp13 + odd0, shift);
		workspace[4 * 8 + column] = descale(tmp13 - odd0, shift);
	}
	let shift = CONST_BITS + PASS1_BITS + 3;
	let clamp_sample = |value: i64| -> u8 { (value + 128).clamp(0, 255) as u8 };
	for row in 0..8 {
		let line = &workspace[row * 8..row * 8 + 8];
		let output_row = &mut output[output_offset + row * output_stride..output_offset + row * output_stride + 8];
		if line[1..].iter().all(|value| *value == 0) {
			let sample = clamp_sample(descale(line[0], PASS1_BITS + 3));
			output_row.fill(sample);
			continue;
		}
		let z2 = line[2];
		let z3 = line[6];
		let z1 = (z2 + z3) * FIX_0_541196100;
		let tmp2 = z1 + z3 * -FIX_1_847759065;
		let tmp3 = z1 + z2 * FIX_0_765366865;
		let tmp0 = (line[0] + line[4]) << CONST_BITS;
		let tmp1 = (line[0] - line[4]) << CONST_BITS;
		let tmp10 = tmp0 + tmp3;
		let tmp13 = tmp0 - tmp3;
		let tmp11 = tmp1 + tmp2;
		let tmp12 = tmp1 - tmp2;
		let (odd0, odd1, odd2, odd3) = inverse_transform_odd(line[7], line[5], line[3], line[1]);
		output_row[0] = clamp_sample(descale(tmp10 + odd3, shift));
		output_row[7] = clamp_sample(descale(tmp10 - odd3, shift));
		output_row[1] = clamp_sample(descale(tmp11 + odd2, shift));
		output_row[6] = clamp_sample(descale(tmp11 - odd2, shift));
		output_row[2] = clamp_sample(descale(tmp12 + odd1, shift));
		output_row[5] = clamp_sample(descale(tmp12 - odd1, shift));
		output_row[3] = clamp_sample(descale(tmp13 + odd0, shift));
		output_row[4] = clamp_sample(descale(tmp13 - odd0, shift));
	}
}

/// 홀수 쪽. (y7, y5, y3, y1 을 받아 tmp0 ~ tmp3)
#[inline]
fn inverse_transform_odd(y7: i64, y5: i64, y3: i64, y1: i64) -> (i64, i64, i64, i64) {
	let z1 = y7 + y1;
	let z2 = y5 + y3;
	let z3 = y7 + y3;
	let z4 = y5 + y1;
	let z5 = (z3 + z4) * FIX_1_175875602;
	let tmp0 = y7 * FIX_0_298631336;
	let tmp1 = y5 * FIX_2_053119869;
	let tmp2 = y3 * FIX_3_072711026;
	let tmp3 = y1 * FIX_1_501321110;
	let z1 = z1 * -FIX_0_899976223;
	let z2 = z2 * -FIX_2_562915447;
	let z3 = z3 * -FIX_1_961570560 + z5;
	let z4 = z4 * -FIX_0_390180644 + z5;
	(tmp0 + z1 + z3, tmp1 + z2 + z4, tmp2 + z2 + z3, tmp3 + z1 + z4)
}

//==============================================================================
/// 성분 하나를 표본 평면으로. (쓰는 블록만, 폭은 블록 x 8)
//==============================================================================
fn inverse_transform_component(component: &Component) -> Vec<u8> {
	let stride = component.used_blocks_per_line * 8;
	let mut plane = vec![0u8; stride * component.used_blocks_per_column * 8];
	let quantization_table = component.quantization_table.unwrap_or([1; 64]);
	for block_row in 0..component.used_blocks_per_column {
		for block_column in 0..component.used_blocks_per_line {
			let block_index = block_row * component.blocks_per_line + block_column;
			let block = &component.coefficients[block_index * 64..block_index * 64 + 64];
			inverse_transform_block(block, &quantization_table, &mut plane, block_row * 8 * stride + block_column * 8, stride);
		}
	}
	plane
}

//==============================================================================
/// 줄이기 전 성분 평면 하나. (역 DCT 결과나 무손실 표본)
//==============================================================================
struct SamplePlane<'a> {
	component: &'a Component,
	plane: &'a [u8],
	stride: usize,
	/// 부드러운 늘리기. (무손실은 블록이 한 표본이라 쓰지 않는다, do_fancy 는 _min_DCT_scaled_size > 1 일 때만)
	is_fancy: bool,
}

//==============================================================================
/// 줄 하나를 늘리는 데 쓰는 자리. (그림마다 한 번 잡아 줄마다 되쓴다)
//==============================================================================
struct RowScratch {
	column_sum: Vec<i32>,
	row: Vec<u8>,
}

//==============================================================================
/// 성분의 출력 줄 y 를 그림 폭으로 늘린다. (jdsample.c 의 고르기와 셈)
//==============================================================================
/// - 가로 두 배(h2v1)는 3:1 에 반올림 +1, +2, 가로세로 두 배(h2v2)는 세로 3:1 합을 가로 3:1 로 섞어 +8, +7 뒤 16 으로 나누고, 세로 두 배(h1v2)는 +1, +2 뒤 4 로 나눈다.
/// - 폭이 2 이하면 h2v1, h2v2 는 되풀이, 그 밖의 정수 배는 되풀이, 정수 배가 아니면 가까운 표본. 위 가장자리는 첫 줄, 아래는 마지막 실제 줄, 오른쪽은 마지막 칸을 되쓴다.
fn upsample_row<'a>(source: &SamplePlane<'a>, y: usize, width: usize, max_horizontal_sampling: usize, max_vertical_sampling: usize, scratch: &'a mut RowScratch) -> &'a [u8] {
	let component = source.component;
	let plane = source.plane;
	let stride = source.stride;
	let source_width = component.downsampled_width;
	let source_height = component.downsampled_height;
	let horizontal_ratio = max_horizontal_sampling / component.horizontal_sampling;
	let vertical_ratio = max_vertical_sampling / component.vertical_sampling;
	let is_integral = max_horizontal_sampling % component.horizontal_sampling == 0 && max_vertical_sampling % component.vertical_sampling == 0;
	let row_at = |source_y: usize| -> &'a [u8] { &plane[source_y.min(source_height - 1) * stride..][..source_width] };
	if !is_integral {
		let source_row = row_at(y * component.vertical_sampling / max_vertical_sampling);
		for (x, output) in scratch.row[..width].iter_mut().enumerate() {
			*output = source_row[(x * component.horizontal_sampling / max_horizontal_sampling).min(source_width - 1)];
		}
		return &scratch.row[..width];
	}
	match (horizontal_ratio, vertical_ratio) {
		(1, 1) => &plane[y * stride..y * stride + width],
		(2, 1) if source.is_fancy && source_width > 2 => {
			let source_row = row_at(y);
			let doubled = &mut scratch.row[..source_width * 2];
			doubled[0] = source_row[0];
			doubled[source_width * 2 - 1] = source_row[source_width - 1];
			for index in 1..source_width {
				doubled[index * 2] = ((source_row[index] as i32 * 3 + source_row[index - 1] as i32 + 1) >> 2) as u8;
				doubled[index * 2 - 1] = ((source_row[index - 1] as i32 * 3 + source_row[index] as i32 + 2) >> 2) as u8;
			}
			&scratch.row[..width]
		}
		(1, 2) if source.is_fancy => {
			let source_y = y >> 1;
			let (neighbor_y, bias) = if y & 1 == 0 { (source_y.saturating_sub(1), 1) } else { ((source_y + 1).min(source_height - 1), 2) };
			let this_row = row_at(source_y);
			let neighbor_row = row_at(neighbor_y);
			for ((output, this), neighbor) in scratch.row[..width].iter_mut().zip(this_row.iter()).zip(neighbor_row.iter()) {
				*output = ((*this as i32 * 3 + *neighbor as i32 + bias) >> 2) as u8;
			}
			&scratch.row[..width]
		}
		(2, 2) if source.is_fancy && source_width > 2 => {
			let source_y = y >> 1;
			let neighbor_y = if y & 1 == 0 { source_y.saturating_sub(1) } else { (source_y + 1).min(source_height - 1) };
			let this_row = row_at(source_y);
			let neighbor_row = row_at(neighbor_y);
			let column_sum = &mut scratch.column_sum[..source_width];
			for ((sum, this), neighbor) in column_sum.iter_mut().zip(this_row.iter()).zip(neighbor_row.iter()) {
				*sum = *this as i32 * 3 + *neighbor as i32;
			}
			let doubled = &mut scratch.row[..source_width * 2];
			doubled[0] = ((column_sum[0] * 4 + 8) >> 4) as u8;
			doubled[source_width * 2 - 1] = ((column_sum[source_width - 1] * 4 + 7) >> 4) as u8;
			for (pair, sums) in doubled[1..source_width * 2 - 1].chunks_exact_mut(2).zip(column_sum.windows(2)) {
				pair[0] = ((sums[0] * 3 + sums[1] + 7) >> 4) as u8;
				pair[1] = ((sums[1] * 3 + sums[0] + 8) >> 4) as u8;
			}
			&scratch.row[..width]
		}
		_ => {
			let source_row = row_at(y / vertical_ratio);
			for (x, output) in scratch.row[..width].iter_mut().enumerate() {
				*output = source_row[(x / horizontal_ratio).min(source_width - 1)];
			}
			&scratch.row[..width]
		}
	}
}

const SCALE_BITS: u32 = 16;
const ONE_HALF: i32 = 1 << (SCALE_BITS - 1);
/// FIX(x) = x x 2^16 + 0.5 를 자른 값. (jdcolor.c)
const FIX_1_40200: i32 = 91881;
const FIX_1_77200: i32 = 116130;
const FIX_0_71414: i32 = 46802;
const FIX_0_34414: i32 = 22554;

//==============================================================================
/// YCbCr 을 RGB 로. (jdcolor.c 의 16 비트 표와 같은 셈을 표 없이 바로 한다, 줄 단위로 자동 벡터화된다)
//==============================================================================
#[inline(always)]
fn ycbcr_to_rgb(y: u8, cb: u8, cr: u8) -> [u8; 3] {
	let y = y as i32;
	let cb = cb as i32 - 128;
	let cr = cr as i32 - 128;
	[
		(y + ((FIX_1_40200 * cr + ONE_HALF) >> SCALE_BITS)).clamp(0, 255) as u8,
		(y + ((-FIX_0_34414 * cb + ONE_HALF - FIX_0_71414 * cr) >> SCALE_BITS)).clamp(0, 255) as u8,
		(y + ((FIX_1_77200 * cb + ONE_HALF) >> SCALE_BITS)).clamp(0, 255) as u8,
	]
}

//==============================================================================
/// 성분 평면을 줄마다 늘려 바로 RGBA 로. (jdcolor.c 의 셈과 블링크 SetPixel<JCS_CMYK>, 늘린 성분 평면을 그림 크기로 따로 두지 않는다)
//==============================================================================
fn convert_rows(sources: &[SamplePlane], width: usize, height: usize, max_horizontal_sampling: usize, max_vertical_sampling: usize, color_transform: ColorTransform) -> Vec<u8> {
	let mut pixels = vec![0u8; width * height * 4];
	let row_capacity = sources.iter().map(|source| source.component.downsampled_width * 2).max().unwrap_or(0).max(width);
	let mut scratch_list: Vec<RowScratch> = sources.iter().map(|source| RowScratch { column_sum: vec![0; source.component.downsampled_width], row: vec![0; row_capacity] }).collect();
	for (y, output_row) in pixels.chunks_exact_mut(width * 4).enumerate() {
		let mut rows: [&[u8]; 4] = [&[]; 4];
		for ((row, source), scratch) in rows.iter_mut().zip(sources.iter()).zip(scratch_list.iter_mut()) {
			*row = upsample_row(source, y, width, max_horizontal_sampling, max_vertical_sampling, scratch);
		}
		let output = output_row.as_chunks_mut::<4>().0;
		match color_transform {
			ColorTransform::Gray => {
				for (pixel, gray) in output.iter_mut().zip(rows[0].iter()) {
					*pixel = [*gray, *gray, *gray, 255];
				}
			}
			ColorTransform::Rgb => {
				for (((pixel, red), green), blue) in output.iter_mut().zip(rows[0].iter()).zip(rows[1].iter()).zip(rows[2].iter()) {
					*pixel = [*red, *green, *blue, 255];
				}
			}
			ColorTransform::YCbCr => {
				for (((pixel, luma), cb), cr) in output.iter_mut().zip(rows[0].iter()).zip(rows[1].iter()).zip(rows[2].iter()) {
					let rgb = ycbcr_to_rgb(*luma, *cb, *cr);
					*pixel = [rgb[0], rgb[1], rgb[2], 255];
				}
			}
			ColorTransform::Cmyk | ColorTransform::Ycck => {
				for (index, pixel) in output.iter_mut().enumerate() {
					let cmy = if color_transform == ColorTransform::Ycck {
						let rgb = ycbcr_to_rgb(rows[0][index], rows[1][index], rows[2][index]);
						[255 - rgb[0], 255 - rgb[1], 255 - rgb[2]]
					}
					else {
						[rows[0][index], rows[1][index], rows[2][index]]
					};
					let black = rows[3][index] as u32;
					*pixel = [(cmy[0] as u32 * black / 255) as u8, (cmy[1] as u32 * black / 255) as u8, (cmy[2] as u32 * black / 255) as u8, 255];
				}
			}
		}
	}
	pixels
}

//==============================================================================
/// EXIF 방향대로 돌리고 뒤집는다. (크롬처럼 그림 요소의 기본 image-orientation: from-image, WebGL texImage2D 도 돌린 그림을 받는다)
/// - 1 그대로, 2 좌우, 3 180 도, 4 위아래, 5 대각(전치), 6 시계 90 도, 7 반대 대각, 8 반시계 90 도. (TIFF 6.0 Orientation 태그)
///
/// # 돌려줌
/// 돌린 폭, 높이, RGBA.
//==============================================================================
pub(crate) fn apply_orientation(width: u32, height: u32, pixels: Vec<u8>, orientation: u8) -> (u32, u32, Vec<u8>) {
	if !(2..=8).contains(&orientation) {
		return (width, height, pixels);
	}
	let source_width = width as usize;
	let source_height = height as usize;
	let is_transposed = orientation >= 5;
	let (output_width, output_height) = if is_transposed { (source_height, source_width) } else { (source_width, source_height) };
	let mut output = vec![0u8; pixels.len()];
	for output_y in 0..output_height {
		for output_x in 0..output_width {
			let (source_x, source_y) = match orientation {
				2 => (source_width - 1 - output_x, output_y),
				3 => (source_width - 1 - output_x, source_height - 1 - output_y),
				4 => (output_x, source_height - 1 - output_y),
				5 => (output_y, output_x),
				6 => (output_y, source_height - 1 - output_x),
				7 => (source_width - 1 - output_y, source_height - 1 - output_x),
				_ => (source_width - 1 - output_y, output_x),
			};
			let source_index = (source_y * source_width + source_x) * 4;
			let output_index = (output_y * output_width + output_x) * 4;
			output[output_index..output_index + 4].copy_from_slice(&pixels[source_index..source_index + 4]);
		}
	}
	(output_width as u32, output_height as u32, output)
}

#[cfg(test)]
mod tests {
	use super::*;

	const HASH_BASE420: u64 = 0x5284fcdc3a0fea3e;
	const PIXEL_BASE420: [u8; 4] = [181, 78, 255, 255];
	const HASH_PROG422: u64 = 0x62235fdab775293c;
	const PIXEL_PROG422: [u8; 4] = [170, 92, 211, 255];
	const HASH_GRAY: u64 = 0x112f5a04a5efb9e4;
	const PIXEL_GRAY: [u8; 4] = [129, 129, 129, 255];
	const HASH_CMYK: u64 = 0x130753f554425bf0;
	const PIXEL_CMYK: [u8; 4] = [175, 78, 204, 255];
	const HASH_LOSSLESS: u64 = 0x91036ce88eb43c26;
	const PIXEL_LOSSLESS: [u8; 4] = [176, 83, 227, 255];

	//==============================================================================
	/// 픽셀 열의 해시를 구한다. (시험용)
	//==============================================================================
	fn hash_pixels(pixels: &[u8]) -> u64 {
		let mut hash = 0xcbf29ce484222325u64;
		for byte in pixels {
			hash ^= *byte as u64;
			hash = hash.wrapping_mul(0x100000001b3);
		}
		hash
	}

	/// 작은 시험 그림(11 x 9)을 풀어 크롬 158 WebGL texImage2D 와 비트까지 같은 결과(FNV-1a 64)를 고정한다.
	/// 기준선 4:2:0, 프로그레시브 4:2:2 와 MCU 마다 재시작, 회색, Adobe CMYK, 무손실 RGB 예측 7.
	#[test]
	fn decode_matches_chrome_pixels() {
		let case_list: [(&[u8], u64, [u8; 4]); 5] = [
			(include_bytes!("../../tests/assets/jpeg/u_base420.jpg"), HASH_BASE420, PIXEL_BASE420),
			(include_bytes!("../../tests/assets/jpeg/u_prog422.jpg"), HASH_PROG422, PIXEL_PROG422),
			(include_bytes!("../../tests/assets/jpeg/u_gray.jpg"), HASH_GRAY, PIXEL_GRAY),
			(include_bytes!("../../tests/assets/jpeg/u_cmyk.jpg"), HASH_CMYK, PIXEL_CMYK),
			(include_bytes!("../../tests/assets/jpeg/u_lossless.jpg"), HASH_LOSSLESS, PIXEL_LOSSLESS),
		];
		for (index, (bytes, expected_hash, expected_pixel)) in case_list.iter().enumerate() {
			let decoded = decode_jpeg(bytes).expect("jpeg");
			assert_eq!((decoded.width, decoded.height, decoded.orientation), (11, 9, 1), "그림 {}", index);
			assert_eq!(&decoded.pixels[..4], expected_pixel, "그림 {}", index);
			assert_eq!(hash_pixels(&decoded.pixels), *expected_hash, "그림 {}", index);
		}
	}

	/// 정수 역 DCT 의 DC 만 있는 블록은 (DC x Q x 4 + 16) >> 5 + 128 이고, 색 표는 jdcolor.c 와 같다.
	#[test]
	fn inverse_transform_and_color_table() {
		let mut block = [0i16; 64];
		block[0] = 10;
		let table = [8u16; 64];
		let mut output = [0u8; 64];
		inverse_transform_block(&block, &table, &mut output, 0, 8);
		assert!(output.iter().all(|value| *value == 138));
		assert_eq!(ycbcr_to_rgb(128, 128, 128), [128, 128, 128]);
		assert_eq!(ycbcr_to_rgb(76, 85, 255), [254, 0, 0]);
		// jdcolor.c 표의 Cr_r_tab[255], Cb_b_tab[0]
		assert_eq!(ycbcr_to_rgb(0, 128, 255), [178, 0, 0]);
		assert_eq!(ycbcr_to_rgb(255, 0, 128), [255, 255, 28]);
	}

	/// 32 비트 역 DCT 가 64 비트 셈과 비트까지 같은지. (넘침 경계 가까이와 그 밖, 64 비트 길로 넘기는 큰 계수까지)
	#[test]
	fn narrow_inverse_transform_matches_wide() {
		let mut seed = 0x2545f4914f6cdd1du64;
		let mut next = move || {
			seed ^= seed << 13;
			seed ^= seed >> 7;
			seed ^= seed << 17;
			seed
		};
		for round in 0..20000 {
			let mut block = [0i16; 64];
			let mut table = [1u16; 64];
			let magnitude = [8i64, 64, 512, 2048, 12000, 32767][round % 6];
			let quantization_limit = [1u64, 4, 16, 255, 65535][(round / 6) % 5];
			for index in 0..64 {
				if index == 0 || next() % 3 == 0 {
					block[index] = ((next() % (2 * magnitude as u64 + 1)) as i64 - magnitude) as i16;
				}
				table[index] = (1 + next() % quantization_limit) as u16;
			}
			let mut narrow = [0u8; 64];
			let mut wide = [0u8; 64];
			inverse_transform_block(&block, &table, &mut narrow, 0, 8);
			inverse_transform_block_wide(&block, &table, &mut wide, 0, 8);
			assert_eq!(narrow, wide, "{:?} {:?}", block, table);
		}
	}

	/// EXIF 방향 읽기와 돌리기. (6 은 시계 90 도, 8 은 반시계 90 도)
	#[test]
	fn exif_orientation() {
		let mut segment = b"Exif\0\0MM\0\x2a\0\0\0\x08\0\x01".to_vec();
		segment.extend([0x01, 0x12, 0x00, 0x03, 0x00, 0x00, 0x00, 0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);
		assert_eq!(read_exif_orientation(&segment), Some(6));
		let pixels: Vec<u8> = (0..6u8).flat_map(|value| [value, 0, 0, 255]).collect();
		let (width, height, rotated) = apply_orientation(3, 2, pixels.clone(), 6);
		assert_eq!((width, height), (2, 3));
		assert_eq!(rotated.iter().step_by(4).copied().collect::<Vec<u8>>(), vec![3, 0, 4, 1, 5, 2]);
		let (_, _, rotated) = apply_orientation(3, 2, pixels, 8);
		assert_eq!(rotated.iter().step_by(4).copied().collect::<Vec<u8>>(), vec![2, 5, 1, 4, 0, 3]);
	}

	/// 크롬이 풀지 못하는 12 비트와 산술 부호, 다 받지 못한 그림은 크기만 읽은 투명한 그림이다.
	#[test]
	fn unsupported_coding_is_transparent() {
		for (marker, precision) in [(0xc0u8, 12u8), (0xc9, 8)] {
			let bytes = [0xff, 0xd8, 0xff, marker, 0x00, 0x0b, precision, 0x00, 0x03, 0x00, 0x05, 0x01, 0x01, 0x11, 0x00, 0xff, 0xd9];
			let decoded = decode_jpeg(&bytes).expect("jpeg");
			assert_eq!((decoded.width, decoded.height), (5, 3));
			assert!(decoded.pixels.iter().all(|value| *value == 0));
		}
		assert!(decode_jpeg(b"\xff\xd8\xff\xd9").is_err());
		// 다 받지 못한 그림(EOI 가 없음, 자료가 잘림)도 크롬 WebGL 처럼 투명하다.
		let bytes = include_bytes!("../../tests/assets/jpeg/u_base420.jpg");
		assert!(decode_jpeg(&bytes[..bytes.len() * 7 / 10]).is_err());
		for length in [bytes.len() - 2, bytes.len() - 40] {
			let decoded = decode_jpeg(&bytes[..length]).expect("jpeg");
			assert_eq!((decoded.width, decoded.height), (11, 9));
			assert!(decoded.pixels.iter().all(|value| *value == 0), "길이 {}", length);
		}
	}
}
