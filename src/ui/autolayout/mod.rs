//==============================================================================
//! 오토레이아웃. (원본 ui/autolayout, Kiwi dual simplex 기반 Cassowary 제약 솔버)
//! - layoutvariable: 솔버가 푸는 미지수 (값을 나눠 쥐는 핸들)
//! - layoutterm, layoutexpression: 계수 * 변수 항과 선형 식
//! - layoutconstraint, layoutrelation, layoutstrength, layoutpriority: 제약과 관계, 강도, UIKit 꼴 우선순위
//! - layoutconstraintaxis: 가로, 세로 축 (UINode 의 hug, 압축 저항 우선순위가 쓴다)
//! - layoutsymbol, layoutsymboltype, layoutrow: 솔버 안의 심볼과 tableau 행
//! - layoutsolver: 제약 솔버 (여러 노드가 나눠 쥐는 핸들)
//! - 원본과 달리 숫자는 f64 이고, 원본이 예외를 던지는 솔버 함수는 `Result<(), LayoutError>` 를 돌려준다.
//==============================================================================
pub mod layoutconstraint;
pub mod layoutconstraintaxis;
pub mod layoutexpression;
pub mod layoutpriority;
pub mod layoutrelation;
pub mod layoutrow;
pub mod layoutsolver;
pub mod layoutstrength;
pub mod layoutsymbol;
pub mod layoutsymboltype;
pub mod layoutterm;
pub mod layoutvariable;
