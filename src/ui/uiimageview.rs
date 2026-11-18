//==============================================================================
// UI 이미지 뷰. (원본 ui/uiimageview.js 의 UIImageView)
// - 표시 전용 위젯. 스프라이트의 이미지 / 색 / 뒤집기 / 출력 방식 / 블렌드 / 나인슬라이스를 노출한다.
// - 원본은 attach 때 호스트에 Sprite 를 붙이고 참조를 든다. 러스트 판은 Sprite 를 품는다. (UILabel 이 Text 를 품는 것과 같은 까닭)
//   품은 Sprite 는 원본처럼 getSprite / getSpriteMut 로 꺼낸다. 이미지 뷰 노드의 getComponent::<Sprite>() 는 None 이다.
// - 원본 setImage 는 이미지 요소와 ImageAsset 을 함께 받는다. 러스트 판은 setImage(Option<Rc<Image>>) 와 setImageAsset 으로 나눴다.
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
	uiView: UIView,
	sprite: Sprite,
}

impl UIImageView {
	//==============================================================================
	// 생성.
	//==============================================================================
	pub fn new() -> UIImageView {
		UIImageView {
			uiView: UIView::new(),
			sprite: Sprite::new(),
		}
	}

	//==============================================================================
	// 이미지와 함께 생성.
	//==============================================================================
	pub fn createWithImage(image: Rc<Image>) -> UIImageView {
		let mut imageView = UIImageView::new();
		imageView.setImage(Some(image));
		imageView
	}

	//==============================================================================
	// 품은 뷰 반환.
	//==============================================================================
	pub fn getUIView(&self) -> &UIView {
		&self.uiView
	}

	//==============================================================================
	// 품은 뷰 반환. (변경용)
	//==============================================================================
	pub fn getUIViewMut(&mut self) -> &mut UIView {
		&mut self.uiView
	}

	//==============================================================================
	// 컨텐트 노드 핸들.
	//==============================================================================
	pub fn getContent(&self) -> Option<NodeId> {
		self.uiView.getContent()
	}

	//==============================================================================
	// 배경색 설정.
	//==============================================================================
	pub fn setBackgroundColor(&mut self, backgroundColor: impl Into<Color>) {
		self.uiView.setBackgroundColor(backgroundColor);
	}

	//==============================================================================
	// 배경색 반환.
	//==============================================================================
	pub fn getBackgroundColor(&self) -> Color {
		self.uiView.getBackgroundColor()
	}

	//==============================================================================
	// 품은 Sprite 반환. (세밀한 제어가 필요할 때)
	//==============================================================================
	pub fn getSprite(&self) -> &Sprite {
		&self.sprite
	}

	//==============================================================================
	// 품은 Sprite 반환. (변경용)
	//==============================================================================
	pub fn getSpriteMut(&mut self) -> &mut Sprite {
		&mut self.sprite
	}

	//==============================================================================
	// 이미지 설정.
	//==============================================================================
	pub fn setImage(&mut self, image: Option<Rc<Image>>) {
		self.sprite.setImage(image);
	}

	//==============================================================================
	// 이미지 애셋으로 이미지 설정. (아직 읽히지 않았으면 비운다)
	//==============================================================================
	pub fn setImageAsset(&mut self, imageAsset: &mut ImageAsset) {
		let image = imageAsset.getImage();
		self.sprite.setImage(image);
	}

	//==============================================================================
	// 이미지 반환.
	//==============================================================================
	pub fn getImage(&self) -> Option<&Rc<Image>> {
		self.sprite.getImage()
	}

	//==============================================================================
	// 이미지 부분 영역 설정.
	//==============================================================================
	pub fn setImageRect(&mut self, imageRect: Rect) {
		self.sprite.setImageRect(imageRect);
	}

	//==============================================================================
	// 색상 설정. (스프라이트 틴트)
	//==============================================================================
	pub fn setColor(&mut self, color: impl Into<Color>) {
		self.sprite.setColor(color);
	}

	//==============================================================================
	// 색상 반환.
	//==============================================================================
	pub fn getColor(&self) -> Color {
		self.sprite.getColor()
	}

	//==============================================================================
	// 좌우 뒤집기 설정.
	//==============================================================================
	pub fn setHorizontalFlip(&mut self, isHorizontalFlip: bool) {
		self.sprite.setHorizontalFlip(isHorizontalFlip);
	}

	//==============================================================================
	// 상하 뒤집기 설정.
	//==============================================================================
	pub fn setVerticalFlip(&mut self, isVerticalFlip: bool) {
		self.sprite.setVerticalFlip(isVerticalFlip);
	}

	//==============================================================================
	// 출력 방식 설정. (simple / sliced / tiled)
	//==============================================================================
	pub fn setDrawMode(&mut self, spriteDrawMode: SpriteDrawMode) {
		self.sprite.setSpriteDrawMode(spriteDrawMode);
	}

	//==============================================================================
	// 블렌드 모드 설정.
	//==============================================================================
	pub fn setBlendMode(&mut self, spriteBlendMode: SpriteBlendMode) {
		self.sprite.setSpriteBlendMode(spriteBlendMode);
	}

	//==============================================================================
	// 나인슬라이스 영역 설정.
	//==============================================================================
	pub fn setNineSlice(&mut self, nineSlice: Rect) {
		self.sprite.setNineSlice(nineSlice);
	}
}

impl Default for UIImageView {
	fn default() -> UIImageView {
		UIImageView::new()
	}
}

impl Component for UIImageView {
	fn getComponentBase(&self) -> &ComponentBase {
		self.uiView.getComponentBase()
	}

	fn getComponentBaseMut(&mut self) -> &mut ComponentBase {
		self.uiView.getComponentBaseMut()
	}

	fn getComponentType(&self) -> &'static str {
		"UIImageView"
	}

	fn attachToTree(&mut self, nodeTree: &mut NodeTree, nodeId: NodeId) {
		self.uiView.createContentNode(nodeTree, nodeId);
	}

	fn isTickNeeded(&self) -> bool {
		false
	}

	fn isDrawnInsideContent(&self) -> bool {
		true
	}

	fn draw(&mut self, transform: &NodeTransform, graphic: &mut Graphic) {
		self.uiView.drawBackground(transform, graphic);
		self.sprite.draw(transform, graphic);
	}
}
