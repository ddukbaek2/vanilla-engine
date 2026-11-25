//==============================================================================
//! "data:" 주소 풀기. (원본은 fetch 와 브라우저가 data: 주소를 그대로 받는다)
//! - 본문의 퍼센트 꼴을 먼저 풀고, 머리가 ;base64 로 끝나면(대소문자 가리지 않음) base64 를 푼다. (fetch 의 data: 주소 처리와 같은 차례)
//! - 형은 "data:" 와 ;base64 사이다. 비었으면 fetch 처럼 text/plain;charset=US-ASCII 다.
//! - 데스크탑 애셋 읽기(Engine::request_asset_bytes, load_image, load_audio_asset)와 타임라인의 data: 그림이 쓴다.
//!   웹은 브라우저 fetch 가 data: 주소를 푼다. (애셋 기준 주소를 앞에 붙이지 않는다)
//==============================================================================

//==============================================================================
/// 푼 data: 주소. (형과 바이트)
//==============================================================================
pub struct DataUrl {
	pub mime_type: String,
	pub bytes: Vec<u8>,
}

//==============================================================================
/// data: 주소인지. (앞의 "data:" 를 대소문자 가리지 않고 본다)
//==============================================================================
pub fn is_data_url(location: &str) -> bool {
	location.len() >= 5 && location.as_bytes()[..5].eq_ignore_ascii_case(b"data:")
}

//==============================================================================
/// data: 주소 풀기. (쉼표가 없거나 base64 가 틀리면 None)
//==============================================================================
pub fn parse_data_url(data_url: &str) -> Option<DataUrl> {
	if !is_data_url(data_url) {
		return None;
	}
	let separator_index = data_url.find(',')?;
	let header = data_url.get(5..separator_index)?.trim();
	let lowercase_header = header.to_ascii_lowercase();
	let body = decode_percent_encoding(&data_url[separator_index + 1..]);
	let (mime_type, bytes) = if lowercase_header.ends_with(";base64") {
		(header[..header.len() - ";base64".len()].trim(), decode_base64(std::str::from_utf8(&body).ok()?)?)
	}
	else {
		(header, body)
	};
	let mime_type = if mime_type.is_empty() { "text/plain;charset=US-ASCII" } else { mime_type };
	Some(DataUrl {
		mime_type: mime_type.to_string(),
		bytes,
	})
}

//==============================================================================
/// base64 풀기. (표준 알파벳, = 채움과 공백은 건너뛴다. 틀린 글자가 있으면 None)
//==============================================================================
pub fn decode_base64(text: &str) -> Option<Vec<u8>> {
	let mut bytes: Vec<u8> = Vec::with_capacity(text.len() * 3 / 4);
	let mut accumulator: u32 = 0;
	let mut bit_count = 0;
	for character in text.bytes() {
		let sextet = match character {
			b'A'..=b'Z' => {
				character - b'A'
			}
			b'a'..=b'z' => {
				character - b'a' + 26
			}
			b'0'..=b'9' => {
				character - b'0' + 52
			}
			b'+' | b'-' => {
				62
			}
			b'/' | b'_' => {
				63
			}
			b'=' | b' ' | b'\n' | b'\r' | b'\t' => {
				continue;
			}
			_ => {
				return None;
			}
		};
		accumulator = (accumulator << 6) | sextet as u32;
		bit_count += 6;
		if bit_count >= 8 {
			bit_count -= 8;
			bytes.push(((accumulator >> bit_count) & 0xff) as u8);
		}
	}
	Some(bytes)
}

//==============================================================================
/// 퍼센트 꼴 풀기. (%XX 를 그 바이트로 바꾸고, 뒤에 16진 두 자리가 없는 % 는 그대로 둔다)
//==============================================================================
pub fn decode_percent_encoding(text: &str) -> Vec<u8> {
	let text_bytes = text.as_bytes();
	let mut bytes: Vec<u8> = Vec::with_capacity(text_bytes.len());
	let mut byte_index = 0;
	while byte_index < text_bytes.len() {
		let current_byte = text_bytes[byte_index];
		if current_byte == b'%' && byte_index + 2 < text_bytes.len() {
			let high_digit = (text_bytes[byte_index + 1] as char).to_digit(16);
			let low_digit = (text_bytes[byte_index + 2] as char).to_digit(16);
			if let (Some(high_digit), Some(low_digit)) = (high_digit, low_digit) {
				bytes.push((high_digit * 16 + low_digit) as u8);
				byte_index += 3;
				continue;
			}
		}
		bytes.push(current_byte);
		byte_index += 1;
	}
	bytes
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn data_url_reads_mime_type_and_body() {
		assert!(is_data_url("DATA:audio/wav;base64,AAAA"));
		assert!(!is_data_url("sounds/data.wav"));
		let data_url = parse_data_url("data:audio/webm;codecs=opus;base64,T2dnUw%3D%3D").expect("data url");
		assert_eq!(data_url.mime_type, "audio/webm;codecs=opus");
		assert_eq!(data_url.bytes, b"OggS".to_vec());
		let text_url = parse_data_url("data:,a%20b").expect("text url");
		assert_eq!(text_url.mime_type, "text/plain;charset=US-ASCII");
		assert_eq!(text_url.bytes, b"a b".to_vec());
		assert!(parse_data_url("data:audio/wav;base64").is_none());
		assert!(parse_data_url("data:audio/wav;base64,@@@@").is_none());
		assert_eq!(decode_percent_encoding("a%20b%zz%4"), b"a b%zz%4".to_vec());
		assert_eq!(decode_base64("SGVsbG8="), Some(b"Hello".to_vec()));
		assert_eq!(decode_base64("SGk"), Some(b"Hi".to_vec()));
		assert_eq!(decode_base64("S*"), None);
	}
}
