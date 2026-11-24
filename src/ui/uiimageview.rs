//==============================================================================
// UI 이미지 뷰. (원본 ui/uiimageview.js 의 UIImageView)
// - 표시 전용 위젯. 스프라이트의 이미지 / 색 / 뒤집기 / 출력 방식 / 블렌드 / 나인슬라이스를 노출한다.
// - 원본은 attach 때 호스트에 Sprite 를 붙이고 참조를 든다. 러스트 판은 Sprite 를 품는다. (UILabel 이 Text 를 품는 것과 같은 까닭)
//   품은 Sprite 는 원본처럼 get_sprite / get_sprite_mut 로 꺼낸다. 이미지 뷰 노드의 get_component::<Sprite>() 는 None 이다.
// - 원본 set_image 는 이미지 요소와 ImageAsset 을 함께 받는다. 러스트 판은 set_image(Option<Rc<Image>>) 와 set_image_asset 으로 나눴다.
// - 배경색(UIView)은 스프라이트보다 먼저 그린다. (원본은 Sprite 가 먼저 붙어 배경이 그림을 덮는다. 기본 배경이 투명이라 보통은 차이가 없다)
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
// UI 이미지 뷰.
//==============================================================================
pub struct UIImageView {
	ui_view: UIView,
	sprite: Sprite,
}

impl UIImageView {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIImageView {
		UIImageView {
			ui_view: UIView::new(),
			sprite: Sprite::new(),
		}
	}

	//==============================================================================
	// 이미지와 함께 생성.
	//==============================================================================
	pub fn create_with_image(image: Rc<Image>) -> UIImageView {
		let mut image_view = UIImageView::new();
		image_view.set_image(Some(image));
		image_view
	}

	//==============================================================================
	// 품은 뷰 반환.
	//==============================================================================
	pub fn get_uiview(&self) -> &UIView {
		&self.ui_view
	}

	//==============================================================================
	// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn get_uiview_mut(&mut self) -> &mut UIView {
		&mut self.ui_view
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn get_content(&self) -> Option<NodeId> {
		self.ui_view.get_content()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn set_background_color(&mut self, background_color: impl Into<Color>) {
		self.ui_view.set_background_color(background_color);
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn get_background_color(&self) -> Color {
		self.ui_view.get_background_color()
	}

	//==============================================================================
	// 품은 Sprite 반환. (세밀한 제어가 필요할 때)
	//==============================================================================
	pub fn get_sprite(&self) -> &Sprite {
		&self.sprite
	}

	//==============================================================================
	// 품은 Sprite 반환. (변경용)
	//==============================================================================
	pub fn get_sprite_mut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}

	//==============================================================================
	// 이미지 설정.
	//==============================================================================
	pub fn set_image(&mut self, image: Option<Rc<Image>>) {
		self.sprite.set_image(image);
	}

	//==============================================================================
	// 이미지 애셋으로 이미지 설정. (아직 읽히지 않았으면 비운다)
	//==============================================================================
	pub fn set_image_asset(&mut self, image_asset: &mut ImageAsset) {
		let image = image_asset.get_image();
		self.sprite.set_image(image);
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn get_image(&self) -> Option<&Rc<Image>> {
		self.sprite.get_image()
	}

	//==============================================================================
	// 이미지 부분 영역 설정.
	//==============================================================================
	pub fn set_image_rect(&mut self, image_rect: Rect) {
		self.sprite.set_image_rect(image_rect);
	}

	//==============================================================================
	// 색상 설정. (스프라이트 틴트)
	//==============================================================================
	pub fn set_color(&mut self, color: impl Into<Color>) {
		self.sprite.set_color(color);
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn get_color(&self) -> Color {
		self.sprite.get_color()
	}

	//==============================================================================
	// 좌우 뒤집기 설정.
	//==============================================================================
	pub fn set_horizontal_flip(&mut self, is_horizontal_flip: bool) {
		self.sprite.set_horizontal_flip(is_horizontal_flip);
	}

	//==============================================================================
	// 상하 뒤집기 설정.
	//==============================================================================
	pub fn set_vertical_flip(&mut self, is_vertical_flip: bool) {
		self.sprite.set_vertical_flip(is_vertical_flip);
	}

	//==============================================================================
	// 출력 방식 설정. (simple / sliced / tiled)
	//==============================================================================
	pub fn set_draw_mode(&mut self, sprite_draw_mode: SpriteDrawMode) {
		self.sprite.set_sprite_draw_mode(sprite_draw_mode);
	}

	//==============================================================================
	// 블렌드 모드 설정.
	//==============================================================================
	pub fn set_blend_mode(&mut self, sprite_blend_mode: SpriteBlendMode) {
		self.sprite.set_sprite_blend_mode(sprite_blend_mode);
	}

	//==============================================================================
	// 나인슬라이스 영역 설정.
	//==============================================================================
	pub fn set_nine_slice(&mut self, nine_slice: Rect) {
		self.sprite.set_nine_slice(nine_slice);
	}

	//==============================================================================
	// 그림 경로 설정. (원본 imageView.imagePath, 품은 스프라이트에 둔다. UI 문서가 적고 읽는다)
	//==============================================================================
	pub fn set_image_path(&mut self, image_path: &str) {
		self.sprite.set_image_path(image_path);
	}

	//==============================================================================
	// 그림 경로 반환.
	//==============================================================================
	pub fn get_image_path(&self) -> &str {
		self.sprite.get_image_path()
	}
}

impl Default for UIImageView {
	fn default() -> UIImageView {
		UIImageView::new()
	}
}

impl Component for UIImageView {
	fn get_component_base(&self) -> &ComponentBase {
		self.ui_view.get_component_base()
	}

	fn get_component_base_mut(&mut self) -> &mut ComponentBase {
		self.ui_view.get_component_base_mut()
	}

	fn get_component_type(&self) -> &'static str {
		"UIImageView"
	}

	fn attach_to_tree(&mut self, node_tree: &mut NodeTree, node_id: NodeId) {
		self.ui_view.create_content_node(node_tree, node_id);
	}

	fn is_tick_needed(&self) -> bool {
		false
	}

	fn is_drawn_inside_content(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.ui_view.draw_background(transform, graphic);
		self.sprite.draw(transform, graphic);
	}
}
