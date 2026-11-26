//==============================================================================
//! UI 이미지 뷰. (원본 ui/uiimageview.js 의 UIImageView)
//! - 표시 전용 위젯. 스프라이트의 이미지 / 색 / 뒤집기 / 출력 방식 / 블렌드 / 나인슬라이스를 노출한다.
//! - 원본은 attach 때 호스트에 Sprite 를 붙이고 참조를 든다. 러스트 판은 Sprite 를 품는다. (UILabel 이 Text 를 품는 것과 같은 까닭)
//!   품은 Sprite 는 원본처럼 get_sprite / get_sprite_mut 로 꺼낸다. 이미지 뷰 노드의 `get_component::<Sprite>()` 는 None 이다.
//!   UI 문서는 원본처럼 품은 Sprite 를 UIImageView 다음 Sprite 로 쓰고 읽는다. (uidocument.rs)
//! - 그림 경로(get_image_path)는 원본 imageView.imagePath 처럼 품은 Sprite 의 경로(원본 Sprite 의 imagePath)와 따로 든다.
//! - 원본 setImage 는 이미지 요소와 ImageAsset 을 함께 받는다. 러스트 판은 set_image(`Option<Rc<Image>>`) 와 set_image_asset 으로 나눴다.
//! - 배경색(UIView)은 스프라이트보다 먼저 그린다. (원본도 UIImageView 가 먼저 붙고 attach 에서 Sprite 를 뒤에 붙여 같은 차례다)
//==============================================================================
use crate::base::color::Color;
use crate::base::rect::Rect;
use crate::core::component::sprite::{Sprite, SpriteBlendMode, SpriteDrawMode};
use crate::core::component::{Component, ComponentBase};
use crate::core::graphic::Graphic;
use crate::core::image::Image;
use crate::core::node::nodetree::{NodeId, NodeTree};
use crate::core::node::worldnode::NodeTransform;
use crate::resource::imageasset::ImageAsset;
use crate::ui::uiview::UIView;
use std::rc::Rc;

//==============================================================================
/// UI 이미지 뷰.
/// - Sprite 를 품고 이미지, 색, 뒤집기, 출력 방식, 블렌드, 나인슬라이스를 넘겨준다. 배경색을 먼저, 스프라이트를 그 위에 그린다.
///
/// # 예제
/// 코드로 만든 그림을 나인슬라이스로 늘려 보이기
/// ```rust
/// use vanilla::{Image, NodeTree, Rect, Sprite, SpriteDrawMode, UIImageView, Vector2};
///
/// let image = Image::create_from_pixels(32, 32, vec![255u8; 32 * 32 * 4]);
/// let mut node_tree = NodeTree::new();
/// let root_id = node_tree.get_root_id();
/// let frame_id = node_tree.create_child(root_id, "frame");
/// node_tree[frame_id].set_content_size(Vector2::create(160.0, 96.0));
/// let image_view = node_tree.add_component(frame_id, UIImageView::create_with_image(image)).unwrap();
/// image_view.set_draw_mode(SpriteDrawMode::Sliced);
/// image_view.set_nine_slice(Rect::create(8.0, 8.0, 8.0, 8.0));
/// assert!(image_view.get_image().is_some());
/// assert_eq!(image_view.get_sprite().get_sprite_draw_mode(), SpriteDrawMode::Sliced);
/// assert!(node_tree[frame_id].get_component::<Sprite>().is_none());
/// ```
//==============================================================================
pub struct UIImageView {
	ui_view: UIView,
	sprite: Sprite,
	/// 그림 경로. (원본 imageView.imagePath, 품은 스프라이트의 경로(원본 Sprite 의 imagePath)와 따로 든다)
	image_path: String,
}

#[crate::reflect(base = ui_view)]
impl UIImageView {
	//==============================================================================
	/// 생성.
	///
	/// # 돌려줌
	/// 이미지가 없는 새 이미지 뷰. (content 노드는 트리에 붙일 때 만든다)
	//==============================================================================
	pub fn new() -> UIImageView {
		UIImageView {
			ui_view: UIView::new(),
			sprite: Sprite::new(),
			image_path: String::new(),
		}
	}

	//==============================================================================
	/// 이미지와 함께 생성.
	///
	/// # 인자
	/// - `image`: 보일 이미지.
	///
	/// # 돌려줌
	/// 그 이미지를 건 새 이미지 뷰.
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> UIImageView {
		let mut image_view = UIImageView::new();
		image_view.set_image(Some(image));
		image_view
	}

	//==============================================================================
	/// 품은 뷰 반환.
	///
	/// # 돌려줌
	/// 품은 UIView 의 참조.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	/// 품은 뷰 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 UIView 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	/// 컨텐트 노드 핸들.
	///
	/// # 돌려줌
	/// content 노드. 트리에 붙기 전이면 None.
	//==============================================================================
	#[reflect(skip)]
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	/// 배경색 설정.
	///
	/// # 인자
	/// - `background_color`: 배경색. (스프라이트보다 먼저 그린다, 알파가 0 이면 칠하지 않는다)
	//==============================================================================
	#[reflect(skip)]
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	/// 배경색 반환.
	///
	/// # 돌려줌
	/// 배경색. (기본은 알파 0 인 흰색)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	/// 품은 Sprite 반환. (세밀한 제어가 필요할 때)
	///
	/// # 돌려줌
	/// 품은 Sprite 의 참조.
	//==============================================================================
	pub fn get_sprite(&self) -> &Sprite {
		&self.sprite
	}

	//==============================================================================
	/// 품은 Sprite 반환. (변경용)
	///
	/// # 돌려줌
	/// 품은 Sprite 의 바꿀 수 있는 참조.
	//==============================================================================
	pub fn get_sprite_mut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}

	//==============================================================================
	/// 이미지 설정.
	///
	/// # 인자
	/// - `image`: 보일 이미지. None 이면 비운다.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.sprite.set_image(image);
	}

	//==============================================================================
	/// 이미지 애셋으로 이미지 설정. (아직 읽히지 않았으면 비운다)
	///
	/// # 인자
	/// - `image_asset`: 이미지를 꺼낼 애셋. (다 읽히기 전이거나 풀기에 실패했으면 이미지가 None 이 된다)
	//==============================================================================
	pub fn set_image_asset(&mut self, image_asset: &mut ImageAsset) {
		let image = image_asset.get_image();
		self.sprite.set_image(image);
	}

	//==============================================================================
	/// 이미지 반환.
	///
	/// # 돌려줌
	/// 건 이미지. 없으면 None.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.sprite.get_image()
	}

	//==============================================================================
	/// 이미지 부분 영역 설정.
	///
	/// # 인자
	/// - `image_rect`: 이미지 안의 그릴 영역. (픽셀, 빈 사각형이면 이미지 전체. Sliced 출력 방식에서는 쓰지 않는다)
	//==============================================================================
	pub fn set_image_rect(&mut self, image_rect: Rect) {
		self.sprite.set_image_rect(image_rect);
	}

	//==============================================================================
	/// 색상 설정. (스프라이트 틴트)
	///
	/// # 인자
	/// - `color`: 스프라이트 색. (이미지가 있으면 알파가 0 보다 클 때 이미지 실루엣 안쪽을 덮는 틴트, 없으면 채우기 색)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.sprite.set_color(color);
	}

	//==============================================================================
	/// 색상 반환.
	///
	/// # 돌려줌
	/// 스프라이트 색.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.sprite.get_color()
	}

	//==============================================================================
	/// 좌우 뒤집기 설정.
	///
	/// # 인자
	/// - `is_horizontal_flip`: true 면 좌우를 뒤집어 그린다.
	//==============================================================================
	pub fn set_horizontal_flip(&mut self, is_horizontal_flip: bool) {
		self.sprite.set_horizontal_flip(is_horizontal_flip);
	}

	//==============================================================================
	/// 상하 뒤집기 설정.
	///
	/// # 인자
	/// - `is_vertical_flip`: true 면 상하를 뒤집어 그린다.
	//==============================================================================
	pub fn set_vertical_flip(&mut self, is_vertical_flip: bool) {
		self.sprite.set_vertical_flip(is_vertical_flip);
	}

	//==============================================================================
	/// 출력 방식 설정. (simple / sliced / tiled)
	///
	/// # 인자
	/// - `sprite_draw_mode`: 출력 방식. (Simple, Sliced, Tiled)
	//==============================================================================
	pub fn set_draw_mode(&mut self, sprite_draw_mode: SpriteDrawMode) {
		self.sprite.set_sprite_draw_mode(sprite_draw_mode);
	}

	//==============================================================================
	/// 블렌드 모드 설정.
	///
	/// # 인자
	/// - `sprite_blend_mode`: 블렌드 모드.
	//==============================================================================
	pub fn set_blend_mode(&mut self, sprite_blend_mode: SpriteBlendMode) {
		self.sprite.set_sprite_blend_mode(sprite_blend_mode);
	}

	//==============================================================================
	/// 나인슬라이스 영역 설정.
	///
	/// # 인자
	/// - `nine_slice`: 나인슬라이스 테두리. (position.x, position.y, size.x, size.y 가 왼쪽, 위, 오른쪽, 아래 두께, 픽셀. 출력 방식이 Sliced 일 때 쓰고, 노드 content 크기가 테두리 합보다 작으면 그리지 않는다)
	//==============================================================================
	pub fn set_nine_slice(&mut self, nine_slice: Rect) {
		self.sprite.set_nine_slice(nine_slice);
	}

	//==============================================================================
	/// 그림 경로 설정. (원본 imageView.imagePath, UI 문서가 적고 읽는다)
	///
	/// # 인자
	/// - `image_path`: 그림 경로. (이미지를 불러오지는 않는다, 품은 Sprite 의 경로와 따로 든다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.image_path = String::from(image_path);
	}

	//==============================================================================
	/// 그림 경로 반환.
	///
	/// # 돌려줌
	/// 그림 경로. (정하지 않았으면 빈 글)
	//==============================================================================
	#[reflect(skip)]
	pub fn get_image_path(&self) -> &str {
		&self.image_path
	}
}

impl Default for UIImageView {
	//==============================================================================
	/// 생성. (UIImageView::new 와 같다)
	//==============================================================================
	fn default() -> UIImageView {
		UIImageView::new()
	}
}

#[crate::reflect]
impl Component for UIImageView {
	//==============================================================================
	/// 품은 기반 컴포넌트. (원본 상위 클래스 UIView 자리, get_component::<UIView>() 로도 찾힌다)
	//==============================================================================
	fn get_base_component(&self) -> Option<&dyn Component> {
		Some(&self.ui_view)
	}

	//==============================================================================
	/// 품은 기반 컴포넌트. (변경용)
	//==============================================================================
	fn get_base_component_mut(&mut self) -> Option<&mut dyn Component> {
		Some(&mut self.ui_view)
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (품은 뷰의 것)
	//==============================================================================
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	//==============================================================================
	/// 컴포넌트 공통 상태 반환. (품은 뷰의 것, 변경용)
	//==============================================================================
	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	//==============================================================================
	/// 컴포넌트 종류 이름 반환. ("UIImageView")
	//==============================================================================
	fn get_component_type(&self) -> &'static str {
		"UIImageView"
	}

	//==============================================================================
	/// 트리에 붙음. (호스트 아래 content 노드를 만든다)
	//==============================================================================
	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	//==============================================================================
	/// tick 이 필요 없다. (false)
	//==============================================================================
	#[reflect(skip)]
	fn is_tick_needed(&self) -> bool {
		false
	}

	//==============================================================================
	/// 노드 content_size 영역 안에만 그린다. (true)
	//==============================================================================
	#[reflect(skip)]
	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	//==============================================================================
	/// 출력. (배경색을 먼저, 품은 Sprite 를 그 위에)
	//==============================================================================
	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
		self.sprite.draw(transform, graphic);
	}
}
