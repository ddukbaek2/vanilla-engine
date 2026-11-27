//==============================================================================
//! 웹소켓 통신 처리기. (JSON 을 주고받는 클라이언트)
//! - 원본 web/websocketclient.js 를 옮긴 것이다. connect / disconnect / send(JSON) / is_connected / set_receive_event 를 따른다.
//! - 소켓은 ewebsock 0.8 이다. 데스크탑은 tungstenite 를 도는 스레드, 웹(wasm)은 브라우저 WebSocket 을 쓴다.
//! - 원본은 브라우저가 onopen / onmessage / onclose 를 불러 준다. 러스트 판은 씬이 매 프레임 tick 을 불러 쌓인 사건을 꺼낸다.
//!   읽은 JSON 은 원본처럼 receive_message 로 넘긴다. (부르는 쪽이 직접 넣을 수도 있다)
//!   ```text
//!     // 씬 create
//!     self.web_socket_client.connect("ws://localhost:8080");
//!     // 씬 tick
//!     self.web_socket_client.tick();
//!     for message in self.web_socket_client.take_received_messages() { ... }
//!     self.web_socket_client.send(&serde_json::json!({ "type": "move", "x": 3 }));
//!   ```
//! - 원본과 다른 점:
//!   - 받은 JSON 은 수신 콜백이 있으면 그것을 부르고, 없으면 모아 두었다가 take_received_messages 로 꺼낸다. (원본에도 넣었다, 최대 RECEIVED_MESSAGE_LIMIT 개, 넘치면 오래된 것부터 버린다)
//!   - disconnect 는 원본처럼 그 자리에서는 닫기만 걸고, 끊긴 것은 다음 tick 에 친다. (원본은 onclose 가 뒤에 와서 그때까지 is_connected 가 참이다.
//!     ewebsock 데스크탑은 이쪽에서 닫으면 Closed 사건을 보내지 않아, 그 tick 에 남은 사건을 꺼낸 뒤 끊는다)
//!   - 오류 사건은 원본처럼 알리고, 끊긴 것으로 친다. (브라우저는 onerror 뒤에 늘 onclose 가 오고, ewebsock 데스크탑은 오류 뒤 스레드가 끝난다)
//!   - get_web_socket 은 ewebsock 의 송신기(WsSender)다.
//!   - 수신 콜백은 `Rc<RefCell>` 에 담는다. get_receive_event 는 그 Rc 를 돌려주고, 부르는 쪽은 borrow_mut 로 빌려 부른다.
//!   - 데스크탑의 wss:// 는 ewebsock 의 "tls" 기능(tungstenite 의 rustls 와 webpki 뿌리 인증서)으로 연다. rustls 의 암호 공급자는 ring 이고 처음 붙을 때 정한다.
//!     엔진의 tls 기능(기본으로 켜짐)일 때만이다. 끄면 데스크탑은 ws:// 만 열린다.
//!   - 바이너리 메시지도 UTF-8 JSON 이면 받는다. (원본에도 넣었다)
//==============================================================================
use crate::misc::storagetext::{self, JsonSurrogateTable};
use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use serde_json::Value;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

/// 수신 콜백이 없을 때 모아 두는 메시지 수 한도.
pub const RECEIVED_MESSAGE_LIMIT: usize = 1024;

/// 수신 콜백. ((받은 JSON))
pub type WebSocketReceiveEvent = Box<dyn FnMut(&Value)>;

//==============================================================================
/// 웹소켓 통신 처리기.
/// - 씬이 매 프레임 tick 을 불러 쌓인 사건을 꺼낸다. 받은 JSON 은 수신 콜백이 있으면 콜백이, 없으면 take_received_messages 가 꺼낸다.
///
/// # 예제
/// 씬에서 접속하고 매 프레임 받은 것을 꺼내 답하기 (네트워크가 있어야 해서 컴파일만)
/// ```rust,no_run
/// use vanilla::{Engine, Scene, SceneBase, WebSocketClient};
///
/// struct LobbyScene {
/// 	scene_base: SceneBase,
/// 	web_socket_client: WebSocketClient,
/// }
///
/// impl Scene for LobbyScene {
/// 	fn get_scene_base(&self) -> &SceneBase {
/// 		&self.scene_base
/// 	}
///
/// 	fn get_scene_base_mut(&mut self) -> &mut SceneBase {
/// 		&mut self.scene_base
/// 	}
///
/// 	fn create(&mut self, _engine: &Engine) {
/// 		self.web_socket_client.connect("ws://localhost:8080");
/// 	}
///
/// 	fn tick(&mut self, _engine: &Engine, _time_delta: f64) {
/// 		self.web_socket_client.tick();
/// 		for message in self.web_socket_client.take_received_messages() {
/// 			if message["type"] == "ping" {
/// 				self.web_socket_client.send(&serde_json::json!({ "type": "pong" }));
/// 			}
/// 		}
/// 		self.web_socket_client.send(&serde_json::json!({ "type": "move", "x": 3 }));
/// 	}
/// }
/// ```
///
/// 받은 JSON 은 콜백이 없으면 모아 두었다가 꺼낸다 (접속 없이 receive_message 로 넣는다)
/// ```rust
/// use vanilla::WebSocketClient;
///
/// let mut web_socket_client = WebSocketClient::new();
/// assert!(!web_socket_client.is_connected());
/// web_socket_client.send(&serde_json::json!({ "ignored": true }));
/// web_socket_client.receive_message(serde_json::json!({ "type": "hello", "id": 1 }));
/// web_socket_client.receive_message(serde_json::json!([1, 2, 3]));
/// let messages = web_socket_client.take_received_messages();
/// assert_eq!(messages.len(), 2);
/// assert_eq!(messages[0]["id"], 1);
/// assert!(web_socket_client.take_received_messages().is_empty());
/// ```
//==============================================================================
pub struct WebSocketClient {
	web_socket: Option<WsSender>,
	web_socket_receiver: Option<WsReceiver>,
	is_connected: bool,
	/// 닫기를 걸어 다음 tick 에 끊을 차례인지. (원본 onclose 를 기다리는 동안)
	is_closing: bool,
	receive_event: Option<Rc<RefCell<WebSocketReceiveEvent>>>,
	received_message_list: VecDeque<Value>,
}

impl WebSocketClient {
	//==============================================================================
	/// 생성. (접속 전)
	///
	/// # 돌려줌
	/// 접속 전이고 수신 콜백이 없는 처리기.
	//==============================================================================
	pub fn new() -> WebSocketClient {
		WebSocketClient {
			web_socket: None,
			web_socket_receiver: None,
			is_connected: false,
			is_closing: false,
			receive_event: None,
			received_message_list: VecDeque::new(),
		}
	}

	//==============================================================================
	/// 접속. (이미 접속돼 있으면 아무것도 하지 않는다. 열리면 다음 tick 들 사이에 is_connected 가 true 가 된다)
	/// - 소켓을 만들지 못하면 표준 에러에 알리고 아무것도 하지 않는다. 열리지 못하면 tick 이 오류 사건을 보고 끊긴 것으로 친다.
	///
	/// # 인자
	/// - `url`: 접속할 주소. (ws:// 나 wss://, 데스크탑의 wss:// 는 엔진의 tls 기능이 켜져 있을 때만)
	//==============================================================================
	pub fn connect(&mut self, url: &str) {
		if self.is_connected {
			return;
		}
		// 데스크탑 wss:// 의 암호 공급자. (이미 정해져 있으면 그대로 둔다)
		#[cfg(all(not(target_arch = "wasm32"), feature = "tls"))]
		let _ = rustls::crypto::ring::default_provider().install_default();
		match ewebsock::connect(url, ewebsock::Options::default()) {
			Ok((web_socket, web_socket_receiver)) => {
				self.web_socket = Some(web_socket);
				self.web_socket_receiver = Some(web_socket_receiver);
			}
			Err(error) => {
				eprintln!("WebSocketClient: {}", error);
			}
		}
	}

	//==============================================================================
	/// 접속 해제. (접속돼 있을 때만)
	//==============================================================================
	pub fn disconnect(&mut self) {
		if !self.is_connected() || self.is_closing {
			return;
		}
		if let Some(web_socket) = self.web_socket.as_mut() {
			web_socket.close();
		}
		self.is_closing = true;
	}

	//==============================================================================
	/// JSON 송신. (접속돼 있을 때만)
	/// - 글 메시지로 보낸다. 접속 전이면 버린다.
	///
	/// # 인자
	/// - `value`: 보낼 JSON 값.
	//==============================================================================
	pub fn send(&mut self, value: &Value) {
		if !self.is_connected() {
			return;
		}
		let json = storagetext::stringify_json(value, &JsonSurrogateTable::new());
		if let Some(web_socket) = self.web_socket.as_mut() {
			web_socket.send(WsMessage::Text(json));
		}
	}

	//==============================================================================
	/// 갱신. (매 프레임 부른다. 쌓인 사건을 꺼내 접속 상태를 바꾸고 받은 JSON 을 넘긴다)
	//==============================================================================
	pub fn tick(&mut self) {
		let mut event_list: Vec<WsEvent> = Vec::new();
		if let Some(web_socket_receiver) = self.web_socket_receiver.as_ref() {
			while let Some(event) = web_socket_receiver.try_recv() {
				event_list.push(event);
			}
		}
		for event in event_list {
			self.handle_event(event);
		}
		if self.is_closing {
			self.close_connection();
		}
	}

	//==============================================================================
	/// 사건 하나 처리. (원본 onopen / onclose / onerror / onmessage)
	//==============================================================================
	fn handle_event(&mut self, event: WsEvent) {
		match event {
			WsEvent::Opened => {
				self.is_connected = true;
			}
			WsEvent::Closed => {
				self.close_connection();
			}
			WsEvent::Error(error_message) => {
				eprintln!("WebSocketClient: {}", error_message);
				self.close_connection();
			}
			WsEvent::Message(message) => {
				match message {
					WsMessage::Text(text) => {
						self.receive_text(&text);
					}
					WsMessage::Binary(bytes) => {
						match String::from_utf8(bytes) {
							Ok(text) => {
								self.receive_text(&text);
							}
							Err(error) => {
								eprintln!("WebSocketClient: {}", error);
							}
						}
					}
					_ => {
						// Ping / Pong / Unknown 은 넘기지 않는다.
					}
				}
			}
		}
	}

	//==============================================================================
	/// 받은 글을 JSON 으로 읽어 넘긴다. (못 읽으면 알리고 버린다, 원본 onmessage 의 JSON.parse)
	//==============================================================================
	fn receive_text(&mut self, text: &str) {
		match serde_json::from_str::<Value>(text) {
			Ok(value) => {
				self.receive_message(value);
			}
			Err(error) => {
				eprintln!("WebSocketClient: {}", error);
			}
		}
	}

	//==============================================================================
	/// 받은 JSON 넘기기. (수신 콜백이 있으면 콜백, 없으면 모아 두었다가 take_received_messages 로 꺼낸다, 원본 receiveMessage)
	/// - 모아 둔 것이 RECEIVED_MESSAGE_LIMIT(1024) 개면 가장 오래된 것을 버린다. 콜백이 이미 빌려져 있으면(콜백 안에서 다시 부르면) 넘기지 않고 버린다.
	///
	/// # 인자
	/// - `value`: 받은 JSON 값. (부르는 쪽이 직접 넣어도 같은 길을 지난다)
	//==============================================================================
	pub fn receive_message(&mut self, value: Value) {
		if let Some(receive_event) = self.get_receive_event() {
			if let Ok(mut receive_function) = receive_event.try_borrow_mut() {
				(*receive_function)(&value);
			}
			return;
		}
		if self.received_message_list.len() >= RECEIVED_MESSAGE_LIMIT {
			self.received_message_list.pop_front();
		}
		self.received_message_list.push_back(value);
	}

	//==============================================================================
	/// 끊긴 것으로 친다. (원본 onclose: 접속 여부 false, 웹소켓 null)
	//==============================================================================
	fn close_connection(&mut self) {
		self.is_connected = false;
		self.is_closing = false;
		self.web_socket = None;
		self.web_socket_receiver = None;
	}

	//==============================================================================
	/// 모아 둔 받은 메시지 꺼내기. (수신 콜백이 없을 때 모인 것, 받은 차례)
	///
	/// # 돌려줌
	/// 모아 둔 JSON 목록. (받은 차례, 꺼내면 비운다)
	//==============================================================================
	pub fn take_received_messages(&mut self) -> Vec<Value> {
		self.received_message_list.drain(..).collect()
	}

	//==============================================================================
	/// 접속 여부 반환.
	///
	/// # 돌려줌
	/// 열림 사건을 받았고 아직 끊기지 않았으면 true. (disconnect 뒤에도 다음 tick 까지는 true)
	//==============================================================================
	pub fn is_connected(&self) -> bool {
		self.is_connected
	}

	//==============================================================================
	/// 웹소켓 송신기 반환. (접속 시도 중이거나 접속돼 있을 때)
	///
	/// # 돌려줌
	/// ewebsock 의 송신기. 접속 전이거나 끊겼으면 None.
	//==============================================================================
	pub fn get_web_socket(&self) -> Option<&WsSender> {
		self.web_socket.as_ref()
	}

	//==============================================================================
	/// 웹소켓 송신기 반환. (변경용, 글이 아닌 메시지를 직접 보낼 때)
	///
	/// # 돌려줌
	/// ewebsock 의 송신기. (변경용) 접속 전이거나 끊겼으면 None.
	//==============================================================================
	pub fn get_web_socket_mut(&mut self) -> Option<&mut WsSender> {
		self.web_socket.as_mut()
	}

	//==============================================================================
	/// 수신 콜백 설정. (None 이면 받은 메시지를 모아 둔다)
	///
	/// # 인자
	/// - `receive_event`: 받은 JSON 을 넘겨받을 함수. None 이면 끄고 모아 둔다.
	///
	/// # 예제
	/// 콜백이 받으면 모으지 않는다 (씬 상태는 `Rc<RefCell>` 로 나눠 쥔다)
	/// ```rust
	/// use std::cell::RefCell;
	/// use std::rc::Rc;
	/// use vanilla::WebSocketClient;
	///
	/// let score_list: Rc<RefCell<Vec<i64>>> = Rc::new(RefCell::new(Vec::new()));
	/// let score_list_for_event = score_list.clone();
	/// let mut web_socket_client = WebSocketClient::new();
	/// web_socket_client.set_receive_event(Some(Box::new(move |value: &serde_json::Value| {
	/// 	if let Some(score) = value["score"].as_i64() {
	/// 		score_list_for_event.borrow_mut().push(score);
	/// 	}
	/// })));
	/// web_socket_client.receive_message(serde_json::json!({ "score": 42 }));
	/// assert_eq!(*score_list.borrow(), vec![42]);
	/// assert!(web_socket_client.take_received_messages().is_empty());
	/// ```
	//==============================================================================
	pub fn set_receive_event(&mut self, receive_event: Option<WebSocketReceiveEvent>) {
		self.receive_event = receive_event.map(|receive_event| Rc::new(RefCell::new(receive_event)));
	}

	//==============================================================================
	/// 수신 콜백 반환. (부를 때는 borrow_mut 로 빌린다)
	///
	/// # 돌려줌
	/// 수신 콜백을 담은 `Rc<RefCell>`. 없으면 None.
	//==============================================================================
	pub fn get_receive_event(&self) -> Option<Rc<RefCell<WebSocketReceiveEvent>>> {
		self.receive_event.clone()
	}
}

impl Default for WebSocketClient {
	//==============================================================================
	/// 접속 전의 처리기. (new 와 같다)
	//==============================================================================
	fn default() -> WebSocketClient {
		WebSocketClient::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;
	use std::cell::RefCell;
	use std::rc::Rc;

	//==============================================================================
	/// 열림 / 닫힘 사건이 접속 여부를 바꾸고, 콜백이 없으면 받은 JSON 을 차례로 모은다. (못 읽는 글은 버린다)
	//==============================================================================
	#[test]
	fn events_update_state_and_queue_messages() {
		let mut web_socket_client = WebSocketClient::new();
		assert!(!web_socket_client.is_connected());
		web_socket_client.send(&json!({ "ignored": true }));
		web_socket_client.handle_event(WsEvent::Opened);
		assert!(web_socket_client.is_connected());
		web_socket_client.handle_event(WsEvent::Message(WsMessage::Text(String::from("{\"type\":\"hello\",\"id\":1}"))));
		web_socket_client.handle_event(WsEvent::Message(WsMessage::Text(String::from("not json"))));
		web_socket_client.handle_event(WsEvent::Message(WsMessage::Binary(b"[1,2,3]".to_vec())));
		web_socket_client.handle_event(WsEvent::Message(WsMessage::Ping(vec![1])));
		assert_eq!(web_socket_client.take_received_messages(), vec![json!({ "type": "hello", "id": 1 }), json!([1, 2, 3])]);
		assert!(web_socket_client.take_received_messages().is_empty());

		// disconnect 는 원본처럼 그 자리에서는 아직 접속돼 있고, 다음 tick 에 끊긴다.
		web_socket_client.disconnect();
		assert!(web_socket_client.is_connected());
		web_socket_client.tick();
		assert!(!web_socket_client.is_connected());
		web_socket_client.handle_event(WsEvent::Opened);
		web_socket_client.handle_event(WsEvent::Closed);
		assert!(!web_socket_client.is_connected());
		assert!(web_socket_client.get_web_socket().is_none());
	}

	//==============================================================================
	/// 수신 콜백이 있으면 콜백이 받고 모으지 않는다. 오류 사건은 끊긴 것으로 친다. 모으는 수에는 한도가 있다.
	//==============================================================================
	#[test]
	fn receive_event_gets_messages_and_error_disconnects() {
		let received_list: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
		let callback_list = received_list.clone();
		let mut web_socket_client = WebSocketClient::new();
		web_socket_client.set_receive_event(Some(Box::new(move |value: &Value| callback_list.borrow_mut().push(value.clone()))));
		assert!(web_socket_client.get_receive_event().is_some());
		web_socket_client.handle_event(WsEvent::Opened);
		web_socket_client.handle_event(WsEvent::Message(WsMessage::Text(String::from("{\"score\":42}"))));
		assert_eq!(*received_list.borrow(), vec![json!({ "score": 42 })]);
		assert!(web_socket_client.take_received_messages().is_empty());
		web_socket_client.handle_event(WsEvent::Error(String::from("connection reset")));
		assert!(!web_socket_client.is_connected());

		web_socket_client.set_receive_event(None);
		for message_index in 0..(RECEIVED_MESSAGE_LIMIT + 5) {
			web_socket_client.handle_event(WsEvent::Message(WsMessage::Text(message_index.to_string())));
		}
		let queued_messages = web_socket_client.take_received_messages();
		assert_eq!(queued_messages.len(), RECEIVED_MESSAGE_LIMIT);
		assert_eq!(queued_messages[0], json!(5));

		// 원본 receiveMessage 처럼 직접 넣어도 같은 길을 지난다.
		web_socket_client.receive_message(json!({ "local": true }));
		assert_eq!(web_socket_client.take_received_messages(), vec![json!({ "local": true })]);
	}

	//==============================================================================
	/// 열린 곳이 없는 로컬 주소에 붙으면 열리지 않고 오류로 끝난다. (실제 ewebsock 스레드, 바깥 네트워크는 쓰지 않는다)
	//==============================================================================
	#[test]
	fn connect_to_closed_local_port_fails() {
		let mut web_socket_client = WebSocketClient::new();
		web_socket_client.connect("ws://127.0.0.1:1");
		assert!(web_socket_client.get_web_socket().is_some());
		assert!(!web_socket_client.is_connected());
		let start_instant = std::time::Instant::now();
		while web_socket_client.get_web_socket().is_some() && start_instant.elapsed().as_secs() < 15 {
			web_socket_client.tick();
			std::thread::sleep(std::time::Duration::from_millis(20));
		}
		assert!(web_socket_client.get_web_socket().is_none());
		assert!(!web_socket_client.is_connected());
		web_socket_client.disconnect();
	}
}
