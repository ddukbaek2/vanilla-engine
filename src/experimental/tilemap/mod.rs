//==============================================================================
//! 타일맵. (초안: 0.4.0 뒤 기능, API 가 바뀔 수 있다, 원본 src/experimental/tilemap/)
//! - Tileset: 그림 한 장의 격자(여백, 간격)나 타일마다의 그림, 타일마다 속성, 종류, 애니메이션, 충돌 모양.
//! - TilemapLayer: 덩이(chunk)로 나눈 칸 저장, 색, 불투명도, 오프셋, 섞는 방식. 끝없는 지도(음수 칸)도 담는다.
//! - Tilemap: 노드에 붙는 컴포넌트. 정사각, 아이소, 엇갈림, 육각 배치의 좌표 바꾸기와 그리기(덩이마다 화면 밖 거르기, 같은 그림끼리 묶어 그리기),
//!   타일 애니메이션 시계, 뒤집기와 돌리기 비트(TileFlag), 칸의 충돌 모양.
//! - RuleTile: 둘레 8 칸 규칙으로 타일을 고르는 자동 타일. (유니티 RuleTile 과 같은 규칙과 변환)
//! - TiledMapReader: Tiled JSON 지도(.tmj)와 타일셋(.tsj) 읽기.
//!
//! ```text
//! let mut tilemap = Tilemap::new();
//! TiledMapReader::read_map(&map_json, &mut tilemap, "orthogonal.tmj", Some(&read_tileset))?;
//! TiledMapReader::attach_images(&mut tilemap, &read_image);
//! node.add_component(tilemap);
//! ```
//==============================================================================
pub mod ruletile;
pub mod tiledmapreader;
pub mod tilemap;
pub mod tilemaplayer;
pub mod tileset;

pub use self::ruletile::{RuleTile, RuleTileNeighbor, RuleTileRule, RuleTileTransform};
pub use self::tiledmapreader::{TiledMap, TiledMapReader, TiledObject, TiledObjectLayer};
pub use self::tilemap::{TileFlag, Tilemap, TilemapOrientation, TilemapRenderMode, TilemapRenderOrder, TilemapStaggerAxis, TilemapStaggerIndex};
pub use self::tilemaplayer::TilemapLayer;
pub use self::tileset::{TileAnimationFrame, TileCollisionShape, TileCollisionShapeKind, TileFillMode, TileRenderSize, Tileset};
