//==============================================================================
// 리소스. (원본 src/resource: Asset 파생)
// - AudioAsset 은 오디오 백엔드(platform/audio)와 붙어 있어서 아직 AssetBase 꼴이 아니다. 읽기는 engine.load_audio_asset.
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
	fn create_test_engine(asset_directory_name: &str) -> (Engine, std::path::PathBuf) {
		let asset_directory_path = std::env::temp_dir().join(asset_directory_name);
		std::fs::create_dir_all(&asset_directory_path).expect("asset directory");
		let mut engine_configuration = EngineConfiguration::new();
		engine_configuration.asset_directory_paths = vec![asset_directory_path.clone()];
		(Engine::new(engine_configuration), asset_directory_path)
	}

	//==============================================================================
	// 데스크탑은 요청한 자리에서 읽혀 끝난다. 글, JSON, 바이너리.
	//==============================================================================
	#[test]
	fn load_text_json_blob_from_file() {
		let (engine, asset_directory_path) = create_test_engine("vanillaAssetTestText");
		std::fs::write(asset_directory_path.join("hello.txt"), "안녕 vanilla").expect("text file");
		std::fs::write(asset_directory_path.join("data.json"), "{\"name\": \"바닐라\", \"count\": 3}").expect("json file");
		let mut text_asset = TextAsset::new();
		text_asset.load(&engine, "hello.txt");
		assert!(text_asset.is_loaded());
		assert_eq!(text_asset.get_text(), Some("안녕 vanilla"));
		let mut json_asset = JsonAsset::new();
		json_asset.load(&engine, "data.json");
		let data = json_asset.get_data().expect("json data");
		assert_eq!(data["name"], "바닐라");
		assert_eq!(data["count"], 3);
		let mut blob_asset = BlobAsset::new();
		blob_asset.load(&engine, "hello.txt");
		let blob = blob_asset.get_blob().expect("blob");
		assert_eq!(blob.len(), "안녕 vanilla".len());
	}

	//==============================================================================
	// 없는 파일은 실패로 끝나고, 씬이 기다리다 멈추지 않는다.
	//==============================================================================
	#[test]
	fn missing_file_finishes_as_failed() {
		let (engine, _asset_directory_path) = create_test_engine("vanillaAssetTestMissing");
		let mut text_asset = TextAsset::new();
		text_asset.load(&engine, "nothing-here.txt");
		assert!(!text_asset.is_loaded());
		let load_handle = text_asset.get_asset_base().get_load_handle().expect("load handle");
		assert!(load_handle.is_failed());
		let mut scene_base = SceneBase::new();
		scene_base.track_asset(&text_asset);
		assert!(scene_base.are_tracked_assets_finished());
	}

	//==============================================================================
	// PNG 바이트를 이미지로 푼다.
	//==============================================================================
	#[test]
	fn decode_png_image() {
		let mut png_bytes: Vec<u8> = Vec::new();
		let pixel_image = image::RgbaImage::from_pixel(3, 2, image::Rgba([255, 0, 0, 255]));
		pixel_image.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).expect("png encode");
		let mut image_asset = ImageAsset::new();
		image_asset.load_from_bytes("red.png", png_bytes);
		let image = image_asset.get_image().expect("decoded image");
		assert_eq!(image.get_width(), 3);
		assert_eq!(image.get_height(), 2);
		let image_size = image_asset.get_image_size();
		assert_eq!(image_size.x, 3.0);
	}

	//==============================================================================
	// 맡긴 애셋이 끝나기 전에는 씬이 기다린다. (웹처럼 나중에 끝나는 손잡이로)
	//==============================================================================
	#[test]
	fn scene_waits_for_pending_asset() {
		let load_handle = crate::core::asset::AssetLoadHandle::create_pending("later.bin");
		let mut scene_base = SceneBase::new();
		scene_base.track_load_handle(&load_handle);
		assert!(!scene_base.are_tracked_assets_finished());
		assert_eq!(scene_base.get_tracked_asset_progress(), 0.0);
		load_handle.finish(Ok(vec![1, 2, 3]));
		assert!(scene_base.are_tracked_assets_finished());
		assert_eq!(scene_base.get_tracked_asset_progress(), 1.0);
	}
}
pub mod assetloader;
