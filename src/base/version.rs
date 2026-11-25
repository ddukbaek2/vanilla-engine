//==============================================================================
// 판. (major.minor.patch, 브랜치가 있으면 major.minor.patch-branch, 원본 base/version.js)
// - 필드를 그대로 열고, 원본의 get_major / set_major 같은 접근자와 getVersionString 도 같은 이름으로 둔다.
// - 브랜치가 글이라 Copy 가 아니다. create 는 원본 기본 인자처럼 브랜치 없이, create_with_branch 는 브랜치를 받는다.
//==============================================================================
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Version {
	pub major: u32,
	pub minor: u32,
	pub patch: u32,
	pub branch: String,
}

impl Version {
	//==============================================================================
	// 생성. (브랜치 없음)
	//==============================================================================
	pub const fn create(major: u32, minor: u32, patch: u32) -> Version {
		Version { major, minor, patch, branch: String::new() }
	}

	//==============================================================================
	// 브랜치를 붙여 생성. (빈 글이면 브랜치 없음)
	//==============================================================================
	pub fn create_with_branch(major: u32, minor: u32, patch: u32, branch: &str) -> Version {
		Version { major, minor, patch, branch: String::from(branch) }
	}

	//==============================================================================
	// 주 판 설정과 반환.
	//==============================================================================
	pub fn set_major(&mut self, major: u32) {
		self.major = major;
	}

	pub fn get_major(&self) -> u32 {
		self.major
	}

	//==============================================================================
	// 부 판 설정과 반환.
	//==============================================================================
	pub fn set_minor(&mut self, minor: u32) {
		self.minor = minor;
	}

	pub fn get_minor(&self) -> u32 {
		self.minor
	}

	//==============================================================================
	// 패치 판 설정과 반환.
	//==============================================================================
	pub fn set_patch(&mut self, patch: u32) {
		self.patch = patch;
	}

	pub fn get_patch(&self) -> u32 {
		self.patch
	}

	//==============================================================================
	// 브랜치 설정과 반환. (빈 글이면 브랜치 없음)
	//==============================================================================
	pub fn set_branch(&mut self, branch: &str) {
		self.branch = String::from(branch);
	}

	pub fn get_branch(&self) -> &str {
		&self.branch
	}

	//==============================================================================
	// 판 문자열. (원본 getVersionString, to_string 과 같다)
	//==============================================================================
	pub fn get_version_string(&self) -> String {
		self.to_string()
	}

	//==============================================================================
	// "major.minor.patch" 문자열. (브랜치가 있으면 "major.minor.patch-branch")
	//==============================================================================
	pub fn to_string(&self) -> String {
		if self.branch.is_empty() {
			format!("{}.{}.{}", self.major, self.minor, self.patch)
		}
		else {
			format!("{}.{}.{}-{}", self.major, self.minor, self.patch, self.branch)
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	//==============================================================================
	// 브랜치가 없으면 세 자리, 있으면 하이픈 뒤에 붙인다. (원본 getVersionString)
	//==============================================================================
	#[test]
	fn version_string_with_branch() {
		assert_eq!(Version::create(0, 3, 2).to_string(), "0.3.2");
		assert_eq!(Version::create_with_branch(0, 4, 0, "experimental").to_string(), "0.4.0-experimental");
		assert_eq!(Version::create_with_branch(1, 2, 3, "").to_string(), "1.2.3");
		let mut version = Version::create(1, 0, 0);
		version.branch = String::from("dev");
		assert_eq!(version.to_string(), "1.0.0-dev");
		version.set_minor(2);
		version.set_branch("");
		assert_eq!(version.get_minor(), 2);
		assert_eq!(version.get_branch(), "");
		assert_eq!(version.get_version_string(), "1.2.0");
	}
}
