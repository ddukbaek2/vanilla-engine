//==============================================================================
//! 읽었지만 아직 올리지 않은 그림 보관소. (원본 DecodedImageStore 의 맡기와 주인 규칙)
//! - ImageAsset 이 읽어 푼 그림(AssetLoadHandle::set_decoded_image)을 약한 참조로 모아 둔다. 씬 로딩 화면 동안 Graphic::prepare_loaded_images 가 올린다.
//! - 맡길 때 주인(그림을 쓸 Graphic 의 GL 컨텍스트)을 적는다. 주인은 ImageAsset::load 를 부를 때 엔진에 든 Graphic 이다.
//!   Graphic 은 제 것만 미리 올린다. 주인을 모르면(그래픽을 내어 준 동안 읽었거나 load_from_bytes) Graphic 이 하나뿐일 때만 미리 올린다. (원본과 같음)
//! - 원본은 브라우저가 일꾼 스레드에서 푼 ImageBitmap 을 맡는다. 러스트는 애셋 손잡이가 이미 푼 그림을 들고 있어 그림만 가리킨다.
//==============================================================================
use crate::core::glcontext::GlContext;
use crate::core::image::Image;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

/// 맡긴 그림 하나. (그림, 주인 GL 컨텍스트의 번호, None 이면 모른다)
struct PendingImage {
	image: Weak<Image>,
	owner: Option<usize>,
}

thread_local! {
	/// 아직 올리지 않은 그림. (약한 참조라 버린 그림은 걸러진다)
	static PENDING_IMAGE_LIST: RefCell<Vec<PendingImage>> = const { RefCell::new(Vec::new()) };
	/// 그림을 올릴 수 있는 주인. (Graphic 이 지어질 때 알린다)
	static OWNER_LIST: RefCell<Vec<Weak<glow::Context>>> = const { RefCell::new(Vec::new()) };
}

//==============================================================================
/// 주인 번호 읽기. (GL 컨텍스트의 자리, 같은 컨텍스트면 같다)
//==============================================================================
pub(crate) fn read_owner(gl: &GlContext) -> usize {
	gl.get_glow_context_unchecked() as *const glow::Context as usize
}

//==============================================================================
/// 그림을 올릴 수 있는 주인 알리기. (Graphic::new 가 부른다)
//==============================================================================
pub(crate) fn register_owner(gl: &Rc<glow::Context>) {
	OWNER_LIST.with(|owner_list| {
		let mut owner_list = owner_list.borrow_mut();
		owner_list.retain(|owner| owner.strong_count() > 0);
		owner_list.push(Rc::downgrade(gl));
	});
}

//==============================================================================
/// 읽은 그림 맡기기. (같은 그림을 두 번 맡기지 않는다)
//==============================================================================
pub(crate) fn store(image: &Rc<Image>, owner: Option<usize>) {
	PENDING_IMAGE_LIST.with(|pending_image_list| {
		let mut pending_image_list = pending_image_list.borrow_mut();
		pending_image_list.retain(|pending_image| pending_image.image.strong_count() > 0);
		if pending_image_list.iter().any(|pending_image| pending_image.image.as_ptr() == Rc::as_ptr(image)) {
			return;
		}
		pending_image_list.push(PendingImage {
			image: Rc::downgrade(image),
			owner,
		});
	});
}

//==============================================================================
/// 그 주인이 올릴 그림 목록. (맡긴 차례, 주인을 모르는 그림은 주인이 하나뿐일 때만)
//==============================================================================
pub(crate) fn get_pending_image_list(owner: usize) -> Vec<Rc<Image>> {
	let live_owner_count = OWNER_LIST.with(|owner_list| owner_list.borrow().iter().filter(|owner| owner.strong_count() > 0).count());
	PENDING_IMAGE_LIST.with(|pending_image_list| {
		let mut pending_image_list = pending_image_list.borrow_mut();
		pending_image_list.retain(|pending_image| pending_image.image.strong_count() > 0);
		pending_image_list
			.iter()
			.filter(|pending_image| match pending_image.owner {
				Some(pending_owner) => {
					pending_owner == owner
				}
				None => {
					live_owner_count == 1
				}
			})
			.filter_map(|pending_image| pending_image.image.upgrade())
			.collect()
	})
}

//==============================================================================
/// 맡긴 그림 빼기. (올렸거나 올리지 못하는 그림)
//==============================================================================
pub(crate) fn release(image: &Rc<Image>) {
	PENDING_IMAGE_LIST.with(|pending_image_list| {
		pending_image_list.borrow_mut().retain(|pending_image| pending_image.image.strong_count() > 0 && pending_image.image.as_ptr() != Rc::as_ptr(image));
	});
}
