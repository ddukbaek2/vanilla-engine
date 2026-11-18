//==============================================================================
// 리소스. (원본 src/resource: Asset 파생)
// - AudioAsset 은 오디오 백엔드(platform/audio)와 붙어 있어서 아직 AssetBase 꼴이 아니다. 읽기는 engine.loadAudioAsset.
//==============================================================================
pub mod animationclip;
pub mod audioasset;
pub mod blobasset;
pub mod bunchasset;
pub mod fontasset;
pub mod imageasset;
pub mod textasset;
pub mod visualasset;

#[cfg(test)]
mod tests {
	use crate::core::asset::Asset;
	use crate::core::engine::{Engine, EngineConfiguration};
	use crate::core::scene::SceneBase;
	use crate::resource::blobasset::BlobAsset;
	use crate::resource::imageasset::ImageAsset;
	use crate::resource::textasset::{JsonAsset, TextAsset};

	//==============================================================================
	// 시험용 엔진. (애셋 폴더를 임시 폴더 하나로)
	//==============================================================================
	fn createTestEngine(assetDirectoryName: &str) -> (Engine, std::path::PathBuf) {
		let assetDirectoryPath = std::env::temp_dir().join(assetDirectoryName);
		std::fs::create_dir_all(&assetDirectoryPath).expect("asset directory");
		let mut engineConfiguration = EngineConfiguration::new();
		engineConfiguration.assetDirectoryPaths = vec![assetDirectoryPath.clone()];
		(Engine::new(engineConfiguration), assetDirectoryPath)
	}

	//==============================================================================
	// 데스크탑은 요청한 자리에서 읽혀 끝난다. 글, JSON, 바이너리.
	//==============================================================================
	#[test]
	fn loadTextJsonBlobFromFile() {
		let (engine, assetDirectoryPath) = createTestEngine("vanillaAssetTestText");
		std::fs::write(assetDirectoryPath.join("hello.txt"), "안녕 vanilla").expect("text file");
		std::fs::write(assetDirectoryPath.join("data.json"), "{\"name\": \"바닐라\", \"count\": 3}").expect("json file");
		let mut textAsset = TextAsset::new();
		textAsset.load(&engine, "hello.txt");
		assert!(textAsset.isLoaded());
		assert_eq!(textAsset.getText(), Some("안녕 vanilla"));
		let mut jsonAsset = JsonAsset::new();
		jsonAsset.load(&engine, "data.json");
		let data = jsonAsset.getData().expect("json data");
		assert_eq!(data["name"], "바닐라");
		assert_eq!(data["count"], 3);
		let mut blobAsset = BlobAsset::new();
		blobAsset.load(&engine, "hello.txt");
		let blob = blobAsset.getBlob().expect("blob");
		assert_eq!(blob.len(), "안녕 vanilla".len());
	}

	//==============================================================================
	// 없는 파일은 실패로 끝나고, 씬이 기다리다 멈추지 않는다.
	//==============================================================================
	#[test]
	fn missingFileFinishesAsFailed() {
		let (engine, _assetDirectoryPath) = createTestEngine("vanillaAssetTestMissing");
		let mut textAsset = TextAsset::new();
		textAsset.load(&engine, "nothing-here.txt");
		assert!(!textAsset.isLoaded());
		let loadHandle = textAsset.getAssetBase().getLoadHandle().expect("load handle");
		assert!(loadHandle.isFailed());
		let mut sceneBase = SceneBase::new();
		sceneBase.trackAsset(&textAsset);
		assert!(sceneBase.areTrackedAssetsFinished());
	}

	//==============================================================================
	// PNG 바이트를 이미지로 푼다.
	//==============================================================================
	#[test]
	fn decodePngImage() {
		let mut pngBytes: Vec<u8> = Vec::new();
		let pixelImage = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]));
		pixelImage.write_to(&mut std::io::Cursor::new(&mut pngBytes), image::ImageFormat::Png).expect("png encode");
		let mut imageAsset = ImageAsset::new();
		imageAsset.loadFromBytes("red.png", pngBytes);
		let image = imageAsset.getImage().expect("decoded image");
		assert_eq!(image.getWidth(), 3);
		assert_eq!(image.getHeight(), 2);
		let imageSize = imageAsset.getImageSize();
		assert_eq!(imageSize.x, 3.0);
	}

	//==============================================================================
	// 맡긴 애셋이 끝나기 전에는 씬이 기다린다. (웹처럼 나중에 끝나는 손잡이로)
	//==============================================================================
	#[test]
	fn sceneWaitsForPendingAsset() {
		let loadHandle = crate::core::asset::AssetLoadHandle::createPending("later.bin");
		let mut sceneBase = SceneBase::new();
		sceneBase.trackLoadHandle(&loadHandle);
		assert!(!sceneBase.areTrackedAssetsFinished());
		assert_eq!(sceneBase.getTrackedAssetProgress(), 0.0);
		loadHandle.finish(Ok(vec![1, 2, 3]));
		assert!(sceneBase.areTrackedAssetsFinished());
		assert_eq!(sceneBase.getTrackedAssetProgress(), 1.0);
	}
}
