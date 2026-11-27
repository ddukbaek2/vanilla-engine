//==============================================================================
//! 바닐라엔진(Vanilla Engine). 크레이트 이름은 vanilla-engine, 코드에서는 vanilla 로 쓴다.
//! - vanilla.js 엔진을 러스트로 옮긴 것이다. 공개 API 는 원본의 쓰임새를 따르고, 이름은 원본 이름을 러스트 규칙으로 바꾼 것이다.
//! - 원본의 WebGL2 렌더러는 OpenGL 3.3 코어(glow)로, 캔버스와 DOM 이벤트는 창(winit)과 GL 컨텍스트(glutin)로 바꿨다.
//!   웹(wasm32)으로 빌드하면 원본처럼 캔버스와 WebGL2, request_animation_frame 으로 돈다. (platform/web.rs)
//! - 이름은 러스트 규칙을 따른다. (함수와 변수는 snake_case, 타입과 열거 값은 UpperCamelCase, 상수는 SCREAMING_SNAKE_CASE, 원본의 setFillColor 는 set_fill_color)
//! - 쓰는 꼴: Scene 트레이트를 구현한 씬(SceneBase 를 품는다)을 만들어 Engine::new(설정).run(첫 씬) 으로 돌린다. 공개 타입은 대부분 크레이트 뿌리(vanilla::)에서 바로 쓴다.
//!
//! # 예제
//! 씬 하나를 만들어 흰 사각형 하나를 그리기 (창이 떠서 컴파일만, examples/quickstart.rs 와 같다)
//! ```rust,no_run
//! use vanilla::{Colors, Engine, EngineConfiguration, Graphic, Rect, Scene, SceneBase, Vector2};
//!
//! struct MainScene {
//! 	scene_base: SceneBase,
//! }
//!
//! impl Scene for MainScene {
//! 	fn get_scene_base(&self) -> &SceneBase {
//! 		&self.scene_base
//! 	}
//!
//! 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
//! 		&mut self.scene_base
//! 	}
//!
//! 	fn pre_draw(&mut self, engine: &Engine, graphic: &mut Graphic) {
//! 		self.scene_base.draw_scene_background(engine, graphic);
//! 	}
//!
//! 	fn draw(&mut self, _engine: &Engine, graphic: &mut Graphic) {
//! 		graphic.set_fill_color(Colors::WHITE);
//! 		graphic.draw_rect(Rect::create(100.0, 100.0, 200.0, 120.0));
//! 	}
//! }
//!
//! fn main() {
//! 	let mut engine_configuration = EngineConfiguration::new();
//! 	engine_configuration.title = String::from("Vanilla Engine");
//! 	engine_configuration.auto_resize_on_window_resize = true;
//! 	engine_configuration.reference_resolution_size = Vector2::create(1280.0, 720.0);
//! 	let engine = Engine::new(engine_configuration);
//! 	let main_scene = MainScene {
//! 		scene_base: SceneBase::new(),
//! 	};
//! 	if let Err(error) = engine.run(Box::new(main_scene)) {
//! 		eprintln!("{}", error);
//! 		std::process::exit(1);
//! 	}
//! }
//! ```
//==============================================================================
#![allow(clippy::too_many_arguments)]

// 반사 매크로가 지은 코드는 엔진을 ::vanilla 경로로 가리킨다. 엔진 크레이트 안에서도 같은 경로가 통하게 자기 자신을 vanilla 로 부른다.
extern crate self as vanilla;

pub mod animation;
pub mod base;
pub mod core;
pub mod effect;
pub mod experimental;
pub mod game;
pub mod graphics3d;
pub mod misc;
pub mod platform;
pub mod resource;
pub mod ui;
pub mod web;

pub use crate::base::color::Color;
pub use crate::base::colors::Colors;
pub use crate::base::format;
pub use crate::base::fsm::{FiniteStateMachine, StateHandlers};
pub use crate::base::math;
pub use crate::base::matrix4::{Matrix4, TransformedPoint};
pub use crate::base::obb::OBB;
pub use crate::base::objectpool::ObjectPool;
pub use crate::base::pivot::Pivot;
pub use crate::base::platform::{BrowserType, Platform, PlatformType};
pub use crate::base::quaternion::Quaternion;
pub use crate::base::rect::Rect;
pub use crate::base::seededrandom::SeededRandom;
pub use crate::base::timer::{Cooldown, RepeatTimer};
pub use crate::base::transformmatrix::TransformMatrix;
pub use crate::base::vector2::Vector2;
pub use crate::base::vector3::Vector3;
pub use crate::base::version::Version;
pub use crate::base::wait;
pub use crate::core::animation::Animation;
pub use crate::core::audiobus::AudioBus;
pub use crate::core::audiomanager::AudioManager;
pub use crate::core::audioplayer::AudioPlayer;
pub use crate::core::asset::{Asset, AssetBase, AssetLoadFuture, AssetLoadHandle, AssetLoadState, AssetType};
pub use crate::core::component::mask::Mask;
pub use crate::core::component::outline::Outline;
pub use crate::core::component::paint::Paint;
pub use crate::core::component::richtext::{RichText, RichTextAttributes, RichTextLayout, RichTextRun, RichTextSegment};
pub use crate::core::component::sprite::{Sprite, SpriteBlendMode, SpriteDrawMode};
pub use crate::core::component::shadowimage::ShadowImage;
pub use crate::core::component::shadowtext::{blur_coverage, compose_shadow_image, ShadowText};
pub use crate::core::component::softshade::{SoftShade, SoftShadeShape};
pub use crate::core::component::text::Text;
pub use crate::core::component::{Component, ComponentBase};
pub use crate::core::reflection::{FromReflectValue, Reflect, ReflectedGetter, ReflectedProperty, ReflectValue, ToReflectValue};
// 반사 매크로. (컴포넌트의 impl 블록과 impl Component 블록에 #[vanilla::reflect] 로 붙인다. 원본의 실행 중 반사 자리)
pub use vanilla_engine_macros::reflect;
pub use crate::core::engine::{Engine, EngineConfiguration};
pub use crate::core::globalengine::EngineAccessError;
pub use crate::core::engine::{PageEvent, PageEventListener};
pub use crate::core::modaldialog::{ModalDialogHandle, ModalDialogKind, ModalDialogResult};
pub use crate::core::frame::Frame;
pub use crate::core::shaderprogram::ShaderProgram;
pub use crate::core::shaderwarmer::{ShaderWarmer, ShaderWarmerSource};
pub use crate::core::fullscreenpass::FullscreenPass;
pub use crate::core::gamepadmanager::{Gamepad, GamepadAnalogStickCode, GamepadButton, GamepadButtonCode, GamepadInputCallback, GamepadInputType, GamepadManager, GamepadTriggerCode};
pub use crate::core::graphic::{BlendMode, Graphic, TextAlign, TextBaseline};
pub use crate::core::glcontext::GlContext;
pub use crate::core::image::Image;
pub use crate::core::canvas2d::{Canvas2D, CanvasFillRule, CanvasGradient, CanvasGradientKind, CanvasImageSource, CanvasLineCap, CanvasLineJoin, CanvasMatrix, CanvasPattern, CanvasPatternRepetition, CanvasRadius, CanvasStyle, CanvasTextAlign, CanvasTextBaseline, ImageData, Path2D};
pub use crate::core::inputmanager::{InputManager, KeyCode, MouseButton, TouchPoint};
pub use crate::core::pointercursor::{CursorValue, PointerCursor};
pub use crate::core::textstringtexturecache::TextRaster;
pub use crate::core::textinput::{TextInput, TextInputEvent, TextInputEventKind, TextInputFieldState, TextInputKey};
pub use crate::core::node::nodetree::{ComponentContext, NodeId, NodeTree, TouchKind};
pub use crate::core::node::worldnode::{NodeTransform, WorldNode};
pub use crate::core::scene::{Scene, SceneBase};
pub use crate::core::scene::SceneId;
pub use crate::core::soundeffectpool::SoundEffectPool;
pub use crate::core::soundsystem::{SoundLoadFuture, SoundLoadHandle, SoundRow, SoundSystem};
pub use crate::core::task::{EngineContext, Promise, PromiseResolver, SceneContext, TaskHandle};
pub use crate::core::timemanager::TimeManager;
pub use crate::core::tween::{easing_function, Tween, TweenId, TweenProperty, TweenValue, TweenValues};
pub use crate::core::viewmanager::{ViewManager, ViewScaleMode};
pub use crate::effect::particlesystem::ParticleSystem;
pub use crate::effect::screeneffect::{ScreenEffect, ScreenEffectRegion, ScreenEffectType, SCREEN_EFFECT_PARAMETER_NAMES};
pub use crate::effect::shadersprite::{ShaderSprite, ShaderSpriteEffect};
pub use crate::effect::trailrenderer::TrailRenderer;
pub use crate::graphics3d::ambientocclusioneffect::AmbientOcclusionEffect;
pub use crate::graphics3d::bloomeffect::BloomEffect;
pub use crate::graphics3d::material::{ImageDescription, ImageDescriptionLoadFuture, ImageDescriptionOptions, ImageSource, Material, MaterialDescription};
pub use crate::graphics3d::rendertarget::{RenderTarget, RenderTargetOptions};
pub use crate::graphics3d::shadowmap::{ShadowAtlas, ShadowAtlasLight, ShadowAtlasLightSource, ShadowAtlasTile, ShadowMap, DEFAULT_SHADOW_ATLAS_RESOLUTION, DEFAULT_SHADOW_ATLAS_TILE_RESOLUTION, SHADOW_ATLAS_MAXIMUM_LIGHTS};
pub use crate::graphics3d::skinnedmodel::{AnimationClip, AnimationPath, Drawable, MeshDescription, MorphTarget, Skin, SkinnedModel, SkinnedModelNode, MORPH_TARGET_MAXIMUM};
pub use crate::graphics3d::skinnedmodelasset::{SkinnedModelAsset, SkinnedModelAssetContent};
pub use crate::graphics3d::skinnedmodelrenderer::{SkinnedModelRenderer, MORPH_GLSL};
pub use crate::graphics3d::subsurfacescatteringeffect::SubsurfaceScatteringEffect;
pub use crate::graphics3d::toon::glbmeshreader::{read_animation_swing, read_glb_arrays, read_glb_node_arrays, read_glb_node_infos, write_skin_pose, ToonAnimation, ToonAnimationChannel, ToonChannelPath, ToonNodeInfo, ToonOutlineOptions, ToonPartArrays, ToonPoseBlend, ToonSkeleton, ToonSkeletonNode, ToonSwingScales};
pub use crate::graphics3d::toon::meshbuilder::{create_box_geometry, create_capsule_geometry, create_cylinder_geometry, create_lathe_geometry, create_octahedron_geometry, create_part_matrix, create_rounded_box_geometry, create_sphere_geometry, create_torus_geometry, read_color_values, MeshBuilder, ToonGeometry, ToonMeshArrays, ToonPartOptions, ToonQuad, DEFAULT_OUTLINE_WIDTH, SKIN_FLOAT_COUNT, VERTEX_FLOAT_COUNT};
pub use crate::graphics3d::toon::toonpostprocess::ToonPostProcess;
pub use crate::graphics3d::toon::toonrenderer::{ToonDrawOptions, ToonFrameStatistics, ToonInstance, ToonMesh, ToonRenderer, TOON_ATTRIBUTE_LOCATIONS, TOON_INSTANCE_FLOAT_COUNT, TOON_MAXIMUM_JOINTS};
pub use crate::misc::beepplayer::{BeepPlayer, BeepTone, BeepWaveform};
pub use crate::misc::camera2d::Camera2D;
pub use crate::misc::cameragestures::CameraGestures;
pub use crate::misc::collision2d::{CircleOverlapPush, Collision2D};
pub use crate::misc::dialogue::{DialogueCurrent, DialogueEntry, DialogueOption, DialogueRunner, DialogueScriptParser};
pub use crate::misc::floatingtext::{FloatingText, FloatingTextOptions};
pub use crate::misc::focusnavigator::{FocusNavigator, FocusNavigatorOptions};
pub use crate::misc::frameprobe::FrameProbe;
pub use crate::misc::grid::{Grid, GridCell};
pub use crate::misc::imagescroller::ImageScroller;
pub use crate::misc::localization::{Localization, LocalizationTable, LocalizationValue};
pub use crate::misc::localstorage::LocalStorage;
pub use crate::misc::nodelayout::{NodeLayout, NodeLayoutFanOptions};
pub use crate::misc::pathfinder::PathFinder;
pub use crate::misc::persistedstore::{PersistedStore, PersistedStoreSubscription};
pub use crate::misc::platformerbody::PlatformerBody;
pub use crate::misc::pointergesture::{PointerGesture, PointerGestureOptions};
pub use crate::misc::shaker::{Shaker, ShakerOptions};
pub use crate::misc::spriteanimator::{SpriteAnimator, SpriteAnimatorPlayOptions};
pub use crate::misc::steering2d::Steering2D;
pub use crate::misc::toucheffect::{TouchEffect, TouchParticle};
pub use crate::misc::typewriter::{Typewriter, TypewriterOptions, TypewriterTarget};
pub use crate::misc::virtualpad::{VirtualPad, VirtualPadState};
pub use crate::resource::audioasset::{AudioAsset, AudioLoadFuture, AudioLoadHandle};
pub use crate::resource::blobasset::BlobAsset;
pub use crate::resource::fontasset::{FontAsset, FontLoadFuture, FontLoadHandle};
pub use crate::resource::imageasset::ImageAsset;
pub use crate::resource::textasset::{JsonAsset, TextAsset};
pub use crate::ui::buttonmotion::{set_button_motion_area, shake_button, ButtonLitHandler, ButtonMotion, ButtonMotionArea, ButtonMotionStyle};
pub use crate::ui::popupmotion::{PopupMotion, PopupMotionEvent, PopupMotionOptions, PopupMotionState};
pub use crate::ui::toastmotion::{read_toast_seconds, ToastApplyText, ToastMotion, ToastPhase};
pub use crate::ui::touchrecognizer::TouchRecognizer;
pub use crate::ui::uibutton::{ButtonState, UIButton, UIButtonEvent};
pub use crate::ui::uichart::{UIBarChart, UILineChart};
pub use crate::ui::uicontextmenu::{UIContextMenu, UIContextMenuHandler, UIContextMenuItem, UIContextMenuOptions};
pub use crate::ui::uicontrol::UIControl;
pub use crate::ui::uidialog::{UIDialog, UIDialogHandler, UIDialogOptions, UIDialogShowOptions};
pub use crate::ui::uidraggable::{UIDraggable, UIDraggableEvent, UIDraggablePositionEvent};
pub use crate::ui::uidropdown::{UIDropdown, UIDropdownEvent, UIDropdownOptions};
pub use crate::ui::uihitarea::UIHitArea;
pub use crate::ui::uiimageview::UIImageView;
pub use crate::ui::uiinputfield::{TextEditBuffer, TextEditKind, UIInputField, UIInputFieldEvent, UIInputFieldMenuAction};
pub use crate::ui::uilabel::UILabel;
pub use crate::ui::uilistview::{UIListView, UIListViewBindItemEvent, UIListViewCreateItemEvent, UIListViewReachEndEvent};
pub use crate::ui::uiprogressview::{ProgressDirection, UIProgressView};
pub use crate::ui::uiprogressring::UIProgressRing;
pub use crate::ui::uiscrollbar::{ScrollBarAxis, UIScrollBar};
pub use crate::ui::uiscrollview::{ScrollMode, UIScrollView};
pub use crate::ui::uislider::{UISlider, UISliderEvent};
pub use crate::ui::uisnapscrollview::UISnapScrollView;
pub use crate::ui::uispinner::{UISpinner, UISpinnerOptions};
pub use crate::ui::uitoast::{UIToast, UIToastOptions};
pub use crate::ui::uitogglebutton::{UIToggleButton, UIToggleButtonEvent};
pub use crate::ui::uiview::UIView;
pub use crate::animation::action::{Action, ActionCallbacks};
pub use crate::animation::animator::{AnimationCondition, AnimationConditionMode, AnimationState, AnimationTransition, Animator, AnimatorParameter};
pub use crate::animation::timeline::{compose_timeline_color, create_bezier_easing, find_timeline_property_definition, parse_timeline_color, resolve_timeline_easing, Timeline, TimelineCompleteHandler, TimelineEasing, TimelineEventHandler, TimelineExternalProperty, TimelineKey, TimelineMarker, TimelineMarkerHandler, TimelineNodeDescription, TimelinePropertyDefinition, TimelinePropertyKind, TimelinePropertyTarget, TimelineSound, TimelineSpriteFrameGrid, TimelineTarget, TimelineTargetResolver, TimelineValue, TIMELINE_EASING_NAMES, TIMELINE_NODE_DEFAULTS, TIMELINE_PROPERTY_DEFINITIONS};
pub use crate::effect::heathazeeffect::HeatHazeEffect;
pub use crate::game::gamescene::GameSceneBase;
pub use crate::misc::indicator::Indicator;
pub use crate::misc::visual::Visual;
pub use crate::resource::bunchasset::BunchAsset;
pub use crate::resource::visualasset::VisualAsset;
pub use crate::web::websocketclient::{WebSocketClient, WebSocketReceiveEvent};
pub use crate::ui::uidocument::{UIDocument, UIDocumentLoadContext, UIDocumentSaveContext, UIDOCUMENT_VERSION};
pub use crate::effect::particlesystem::{ParticleEmitterShape, ParticleRenderShape};
pub use crate::core::component::text::TextWrapMode;
pub use crate::core::inputmanager::{InputDevice, TouchPhase};
pub use crate::core::inputactions::{InputActionSource, InputActions};
pub use crate::core::gamepadmanager::{read_gamepad_kind, GamepadKind};
pub use crate::graphics3d::camera3d::{Camera3D, ProjectedPoint};
pub use crate::graphics3d::toon::toondrawqueue::ToonDrawQueue;
pub use crate::graphics3d::toon::toonview3d::{ToonView3D, ToonViewDrawCallback, ToonViewFallbackDrawCallback, ToonViewOutput};
pub use crate::graphics3d::toon::meshbuilder::{create_faceted_chunk_geometry, ToonMeshPiece};
pub use crate::misc::graphicsqualitygovernor::{GraphicsQuality, GraphicsQualityGovernor};
pub use crate::misc::fixedstepper::FixedStepper;
pub use crate::misc::localstorage::LocalStorageBackend;
pub use crate::resource::assetloader::{AssetLoadFailure, AssetLoader, AssetLoaderCompleteHandler, AssetLoaderLoadFuture, AssetLoaderLoadHandle, AssetLoaderProgressHandler};
pub use crate::misc::loadingprogress::LoadingProgress;
pub use crate::game::gamescene::LoadingDrawer;
pub use crate::misc::buildinfo::{BuildInfo, BuildInfoLoadFuture, BuildInfoLoadHandle};
pub use crate::platform::document::Document;
pub use crate::graphics3d::toon::toonscreenoutline::{read_jump_flood_steps, ToonLineDrawOptions, ToonOutlineWidthUnit, ToonScreenOutline, TOON_SCREEN_OUTLINE_MAXIMUM_LINE_ID};
pub use crate::graphics3d::toon::toonvfx::{ToonBeamOptions, ToonDecalOptions, ToonFireballOptions, ToonGlintOptions, ToonLightningOptions, ToonParticleOptions, ToonPlumeOptions, ToonRibbonOptions, ToonShockwaveOptions, ToonVfx, ToonVfxCounts, ToonVfxDecalMode, ToonVfxLook, ToonVfxRange, ToonVfxShape, ToonVfxSlot, TOON_VFX_DEFAULT_PARTICLE_CAPACITY, TOON_VFX_MAXIMUM_RAMPS};
pub use crate::ui::uiscale::{UIScale, UIScaleOptions};
pub use crate::ui::uiwindow::{UIWindow, UIWindowBehavior, UIWindowDefaultBehavior, UIWindowEvent, UIWindowParts};
pub use crate::ui::uiwindowlayer::{UIWindowChangeEvent, UIWindowLayer};
pub use crate::ui::uiconfirmwindow::{UIConfirmWindow, UIConfirmWindowOptions};
pub use crate::game::gamescene::{PlayInput, ScreenFadeDarkStarter, ScreenFadeEvent, ScreenFadeHold};
pub use crate::core::viewmanager::SafeAreaInsets;
pub use crate::core::viewmanager::SafeAreaSide;
pub use crate::resource::spriteatlas::{SpriteAtlas, SpriteAtlasLoadFuture, SpriteAtlasLoadHandle};
pub use crate::graphics3d::humanskinrenderer::{HumanSkinRenderer, SKIN_SHADING_EYE, SKIN_SHADING_FACIALHAIR, SKIN_SHADING_FLUID, SKIN_SHADING_HAIR, SKIN_SHADING_OCCLUSION, SKIN_SHADING_SKIN, SKIN_TEXTURE_UNIT_AUXILIARY, SKIN_TEXTURE_UNIT_DETAILHEIGHT, SKIN_TEXTURE_UNIT_LAYERCOLOR, SKIN_TEXTURE_UNIT_LAYERNORMAL, SKIN_TEXTURE_UNIT_MASKATLAS, SKIN_TEXTURE_UNIT_MICRO, WRINKLE_CHANNEL_COUNT};
pub use crate::animation::action::ActionConditionCallbacks;
pub use crate::graphics3d::toon::toonrenderer::extract_frustum_planes;
pub use crate::graphics3d::toon::toonscreenoutline::ToonLineTarget;
pub use crate::graphics3d::toon::toonvfx::ToonFlashOptions;
pub use crate::graphics3d::toon::toonvfx::ToonRampColor;
pub use crate::web::pane::{Pane, PaneDirection, PaneOptions, PaneResizableEdges, PaneResizeCallback, PaneSize, PaneStyle, PaneStyleOptions, PaneTheme};
#[cfg(not(target_arch = "wasm32"))]
pub use crate::web::pane::{PaneClickCallback, PaneElement};
pub use crate::game::gamescene::{GameScene, PlayInputHandler, ScreenFadeHoldEvent};
pub use crate::ui::autolayout::layoutconstraint::LayoutConstraint;
pub use crate::ui::autolayout::layoutconstraintaxis::LayoutConstraintAxis;
pub use crate::ui::autolayout::layoutexpression::LayoutExpression;
pub use crate::ui::autolayout::layoutpriority::LayoutPriority;
pub use crate::ui::autolayout::layoutrelation::LayoutRelation;
pub use crate::ui::autolayout::layoutsolver::{LayoutError, LayoutSolver};
pub use crate::ui::autolayout::layoutstrength::LayoutStrength;
pub use crate::ui::autolayout::layoutterm::LayoutTerm;
pub use crate::ui::autolayout::layoutvariable::LayoutVariable;
pub use crate::ui::uinode::{LayoutMargins, UINode, NO_INTRINSIC_METRIC};
pub use crate::ui::uiscene::{UIScene, UISceneBase};
pub use crate::ui::buttonmotion::attach_button_motion;
pub use crate::ui::toastmotion::attach_toast_motion;
pub use crate::misc::devtools::DevTools;
pub use crate::core::component::text::{count_graphemes, split_graphemes, split_words, TextWrappedSize};
pub use crate::core::graphic::{detect_text_direction, TextDirection};
pub use crate::core::textshaping::{FontFaceDescriptors, ShapingDirection};
pub use crate::core::textstringtexturecache::TextMetrics;
pub use crate::misc::dynamicfont::{DynamicFont, GlyphInfo};
pub use crate::misc::languagepack;
pub use crate::misc::languagepack::{LanguageEntry, LanguageLoadFuture, LanguageLoadHandle, LanguageLoadedAsyncHandler};
pub use crate::misc::uifont;
pub use crate::misc::uifont::{UiFontLoadFuture, UiFontLoadHandle, UiFontOptions, UiFontTextLoadFuture, UiFontTextLoadHandle};
pub use crate::core::soundsystem::{read_sound_system, set_sound_system};
pub use crate::base::platform::{LoadedFile, ResourceUsage};
pub use crate::base::identifier::Identifier;
pub use crate::core::engine::FrameCaptureHandler;
pub use crate::base::fsm::StateChangeRequester;
pub use crate::graphics3d::fbxloader::{FbxLoadFuture, FbxLoadHandle};
pub use crate::graphics3d::material::ImageDescriptionLoadHandle;
pub use crate::graphics3d::skinnedmodel::{AnimationLoadFuture, AnimationLoadHandle, SkinnedModelLoadHandle};
pub use crate::graphics3d::multilevelbloomeffect::MultiLevelBloomEffect;
pub use crate::graphics3d::toon::toonpostprocess::ToonPostProcessOptions;
pub use crate::graphics3d::toon::toonvfx::ToonBlobShadowOptions;
pub use crate::base::identifier::create_guid;
pub use crate::core::component::text::slice_graphemes;
pub use crate::core::graphic::ImageSmoothingQuality;
pub use crate::graphics3d::toon::toonlightset::{ToonLightColorUsage, ToonLightModel, ToonLightSet, ToonPointLight, ToonRimLight, ToonShade, ToonShadeRamp, ToonShadeRimLight, ToonSpecular, TOON_MAXIMUM_BAND_STEPS, TOON_MAXIMUM_POINT_LIGHTS};
pub use crate::graphics3d::toon::toonmaterial::{ToonBlendMode, ToonBufferUsage, ToonMatcapBlend, ToonInstanceAttribute, ToonInstanceLayout, ToonMaterial, ToonMaterialChunks, TOON_FIRST_INSTANCE_ATTRIBUTE_LOCATION, TOON_INSTANCE_MATRIX_NAME, TOON_INSTANCE_TINT_NAME, TOON_MAXIMUM_INSTANCE_ATTRIBUTES};
pub use crate::graphics3d::toon::toonrenderer::{compose_toon_program_sources, create_reflection_matrix, read_frame_camera, read_matrix_determinant, read_camera_roll, read_outline_vertex_data, read_uv_data, read_view_rotation, ToonBeforeDrawCallback, ToonFrameCamera, ToonInstanceBuffer, ToonProgramKind, ToonProgramSources, ToonTextureOptions, TOON_MAXIMUM_MATERIAL_TEXTURES};
pub use crate::graphics3d::toon::toondrawqueue::{ToonDrawStage, ToonPassCallback};
pub use crate::graphics3d::toon::meshbuilder::ToonMaterialRange;
pub use crate::graphics3d::toon::glbmeshreader::{decode_glb_image, read_glb_images, ToonGlbImage};
