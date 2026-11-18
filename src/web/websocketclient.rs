//==============================================================================
// 웹소켓 통신 처리기. (JSON 을 주고받는 클라이언트)
// - 원본 web/websocketclient.js 를 옮긴 것이다. connect / disconnect / send(JSON) / isConnected / setReceiveEvent 를 따른다.
// - 소켓은 ewebsock 0.8 이다. 데스크탑은 tungstenite 를 도는 스레드, 웹(wasm)은 브라우저 WebSocket 을 쓴다.
// - 원본은 브라우저가 onopen / onmessage / onclose 를 불러 준다. 러스트 판은 씬이 매 프레임 tick 을 불러 쌓인 사건을 꺼낸다.
//     // 씬 create
//     self.webSocketClient.connect("ws://localhost:8080");
//     // 씬 tick
//     self.webSocketClient.tick();
//     for message in self.webSocketClient.takeReceivedMessages() { ... }
//     self.webSocketClient.send(&serde_json::json!({ "type": "move", "x": 3 }));
// - 원본과 다른 점:
//   - 받은 JSON 은 수신 콜백이 있으면 그것을 부르고(원본과 같음), 없으면 버리지 않고 모아 두었다가 takeReceivedMessages 로 꺼낸다.
//     (러스트 콜백은 씬을 빌려 들 수 없어서 꺼내 쓰는 길을 더했다. 최대 RECEIVED_MESSAGE_LIMIT 개, 넘치면 오래된 것부터 버린다)
//   - disconnect 는 원본처럼 onclose 를 기다리지 않고 그 자리에서 끊긴 것으로 친다. (ewebsock 데스크탑은 이쪽에서 닫으면 Closed 사건을 보내지 않는다)
//   - 오류 사건은 원본처럼 알리고, 끊긴 것으로 친다. (브라우저는 onerror 뒤에 늘 onclose 가 오고, ewebsock 데스크탑은 오류 뒤 스레드가 끝난다)
//   - 원본 getReceiveEvent 는 실수로 웹소켓을 돌려준다. 러스트 판은 수신 콜백을 돌려준다.
//   - getWebSocket 은 ewebsock 의 송신기(WsSender)다.
//   - 데스크탑에서 wss:// 주소는 ewebsock 의 "tls" 기능을 켜야 열린다. (기본은 꺼 두었다)
//   - 바이너리 메시지도 UTF-8 JSON 이면 받는다. (원본은 Blob 을 JSON.parse 하다 실패한다)
//==============================================================================
use ewebsock::{WsEvent, WsMessage, WsReceiver, WsSender};
use serde_json::Value;
use std::collections::VecDeque;

// 수신 콜백이 없을 때 모아 두는 메시지 수 한도.
pub const RECEIVED_MESSAGE_LIMIT: usize = 1024;

// 수신 콜백. ((받은 JSON))
pub type WebSocketReceiveEvent = Box<dyn FnMut(&Value)>;

//==============================================================================
// 웹소켓 통신 처리기.
//==============================================================================
pub struct WebSocketClient {
	webSocket: Option<WsSender>,
	webSocketReceiver: Option<WsReceiver>,
	isConnected: bool,
	receiveEvent: Option<WebSocketReceiveEvent>,
	receivedMessageList: VecDeque<Value>,
}

impl WebSocketClient {
	//==============================================================================
	// 생성. (접속 전)
	//==============================================================================
	pub fn new() -> WebSocketClient {
		WebSocketClient {
			webSocket: None,
			webSocketReceiver: None,
			isConnected: false,
			receiveEvent: None,
			receivedMessageList: VecDeque::new(),
		}
	}

	//==============================================================================
	// 접속. (이미 접속돼 있으면 아무것도 하지 않는다. 열리면 다음 tick 들 사이에 isConnected 가 true 가 된다)
	//==============================================================================
	pub fn connect(&mut self, url: &str) {
		if self.isConnected {
			return;
		}
		match ewebsock::connect(url, ewebsock::Options::default()) {
			Ok((webSocket, webSocketReceiver)) => {
				self.webSocket = Some(webSocket);
				self.webSocketReceiver = Some(webSocketReceiver);
			}
			Err(error) => {
				eprintln!("WebSocketClient: {}", error);
			}
		}
	}

	//==============================================================================
	// 접속 해제. (접속돼 있을 때만)
	//==============================================================================
	pub fn disconnect(&mut self) {
		if !self.isConnected() {
			return;
		}
		if let Some(webSocket) = self.webSocket.as_mut() {
			webSocket.close();
		}
		self.closeConnection();
	}

	//==============================================================================
	// JSON 송신. (접속돼 있을 때만)
	//==============================================================================
	pub fn send(&mut self, value: &Value) {
		if !self.isConnected() {
			return;
		}
		let json = match serde_json::to_string(value) {
			Ok(json) => {
				json
			}
			Err(error) => {
				eprintln!("WebSocketClient: {}", error);
				return;
			}
		};
		if let Some(webSocket) = self.webSocket.as_mut() {
			webSocket.send(WsMessage::Text(json));
		}
	}

	//==============================================================================
	// 갱신. (매 프레임 부른다. 쌓인 사건을 꺼내 접속 상태를 바꾸고 받은 JSON 을 넘긴다)
	//==============================================================================
	pub fn tick(&mut self) {
		let mut eventList: Vec<WsEvent> = Vec::new();
		if let Some(webSocketReceiver) = self.webSocketReceiver.as_ref() {
			while let Some(event) = webSocketReceiver.try_recv() {
				eventList.push(event);
			}
		}
		for event in eventList {
			self.handleEvent(event);
		}
	}

	//==============================================================================
	// 사건 하나 처리. (원본 onopen / onclose / onerror / onmessage)
	//==============================================================================
	fn handleEvent(&mut self, event: WsEvent) {
		match event {
			WsEvent::Opened => {
				self.isConnected = true;
			}
			WsEvent::Closed => {
				self.closeConnection();
			}
			WsEvent::Error(errorMessage) => {
				eprintln!("WebSocketClient: {}", errorMessage);
				self.closeConnection();
			}
			WsEvent::Message(message) => {
				match message {
					WsMessage::Text(text) => {
						self.receiveText(&text);
					}
					WsMessage::Binary(bytes) => {
						match String::from_utf8(bytes) {
							Ok(text) => {
								self.receiveText(&text);
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
	// 받은 글을 JSON 으로 읽어 넘긴다. (콜백이 있으면 콜백, 없으면 모아 둔다. 못 읽으면 알리고 버린다)
	//==============================================================================
	fn receiveText(&mut self, text: &str) {
		let value: Value = match serde_json::from_str(text) {
			Ok(value) => {
				value
			}
			Err(error) => {
				eprintln!("WebSocketClient: {}", error);
				return;
			}
		};
		if let Some(receiveEvent) = self.receiveEvent.as_mut() {
			receiveEvent(&value);
			return;
		}
		if self.receivedMessageList.len() >= RECEIVED_MESSAGE_LIMIT {
			self.receivedMessageList.pop_front();
		}
		self.receivedMessageList.push_back(value);
	}

	//==============================================================================
	// 끊긴 것으로 친다. (원본 onclose: 접속 여부 false, 웹소켓 null)
	//==============================================================================
	fn closeConnection(&mut self) {
		self.isConnected = false;
		self.webSocket = None;
		self.webSocketReceiver = None;
	}

	//==============================================================================
	// 모아 둔 받은 메시지 꺼내기. (수신 콜백이 없을 때 모인 것, 받은 차례)
	//==============================================================================
	pub fn takeReceivedMessages(&mut self) -> Vec<Value> {
		self.receivedMessageList.drain(..).collect()
	}

	//==============================================================================
	// 접속 여부 반환.
	//==============================================================================
	pub fn isConnected(&self) -> bool {
		self.isConnected
	}

	//==============================================================================
	// 웹소켓 송신기 반환. (접속 시도 중이거나 접속돼 있을 때)
	//==============================================================================
	pub fn getWebSocket(&self) -> Option<&WsSender> {
		self.webSocket.as_ref()
	}

	//==============================================================================
	// 웹소켓 송신기 반환. (변경용, 글이 아닌 메시지를 직접 보낼 때)
	//==============================================================================
	pub fn getWebSocketMut(&mut self) -> Option<&mut WsSender> {
		self.webSocket.as_mut()
	}

	//==============================================================================
	// 수신 콜백 설정. (None 이면 받은 메시지를 모아 둔다)
	//==============================================================================
	pub fn setReceiveEvent(&mut self, receiveEvent: Option<WebSocketReceiveEvent>) {
		self.receiveEvent = receiveEvent;
	}

	//==============================================================================
	// 수신 콜백 반환.
	//==============================================================================
	pub fn getReceiveEvent(&mut self) -> Option<&mut WebSocketReceiveEvent> {
		self.receiveEvent.as_mut()
	}
}

impl Default for WebSocketClient {
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
	// 열림 / 닫힘 사건이 접속 여부를 바꾸고, 콜백이 없으면 받은 JSON 을 차례로 모은다. (못 읽는 글은 버린다)
	//==============================================================================
	#[test]
	fn eventsUpdateStateAndQueueMessages() {
		let mut webSocketClient = WebSocketClient::new();
		assert!(!webSocketClient.isConnected());
		webSocketClient.send(&json!({ "ignored": true }));
		webSocketClient.handleEvent(WsEvent::Opened);
		assert!(webSocketClient.isConnected());
		webSocketClient.handleEvent(WsEvent::Message(WsMessage::Text(String::from("{\"type\":\"hello\",\"id\":1}"))));
		webSocketClient.handleEvent(WsEvent::Message(WsMessage::Text(String::from("not json"))));
		webSocketClient.handleEvent(WsEvent::Message(WsMessage::Binary(b"[1,2,3]".to_vec())));
		webSocketClient.handleEvent(WsEvent::Message(WsMessage::Ping(vec![1])));
		assert_eq!(webSocketClient.takeReceivedMessages(), vec![json!({ "type": "hello", "id": 1 }), json!([1, 2, 3])]);
		assert!(webSocketClient.takeReceivedMessages().is_empty());
		webSocketClient.handleEvent(WsEvent::Closed);
		assert!(!webSocketClient.isConnected());
		assert!(webSocketClient.getWebSocket().is_none());
	}

	//==============================================================================
	// 수신 콜백이 있으면 콜백이 받고 모으지 않는다. 오류 사건은 끊긴 것으로 친다. 모으는 수에는 한도가 있다.
	//==============================================================================
	#[test]
	fn receiveEventGetsMessagesAndErrorDisconnects() {
		let receivedList: Rc<RefCell<Vec<Value>>> = Rc::new(RefCell::new(Vec::new()));
		let callbackList = receivedList.clone();
		let mut webSocketClient = WebSocketClient::new();
		webSocketClient.setReceiveEvent(Some(Box::new(move |value: &Value| callbackList.borrow_mut().push(value.clone()))));
		assert!(webSocketClient.getReceiveEvent().is_some());
		webSocketClient.handleEvent(WsEvent::Opened);
		webSocketClient.handleEvent(WsEvent::Message(WsMessage::Text(String::from("{\"score\":42}"))));
		assert_eq!(*receivedList.borrow(), vec![json!({ "score": 42 })]);
		assert!(webSocketClient.takeReceivedMessages().is_empty());
		webSocketClient.handleEvent(WsEvent::Error(String::from("connection reset")));
		assert!(!webSocketClient.isConnected());

		webSocketClient.setReceiveEvent(None);
		for messageIndex in 0..(RECEIVED_MESSAGE_LIMIT + 5) {
			webSocketClient.handleEvent(WsEvent::Message(WsMessage::Text(messageIndex.to_string())));
		}
		let queuedMessages = webSocketClient.takeReceivedMessages();
		assert_eq!(queuedMessages.len(), RECEIVED_MESSAGE_LIMIT);
		assert_eq!(queuedMessages[0], json!(5));
	}

	//==============================================================================
	// 열린 곳이 없는 로컬 주소에 붙으면 열리지 않고 오류로 끝난다. (실제 ewebsock 스레드, 바깥 네트워크는 쓰지 않는다)
	//==============================================================================
	#[test]
	fn connectToClosedLocalPortFails() {
		let mut webSocketClient = WebSocketClient::new();
		webSocketClient.connect("ws://127.0.0.1:1");
		assert!(webSocketClient.getWebSocket().is_some());
		assert!(!webSocketClient.isConnected());
		let startInstant = std::time::Instant::now();
		while webSocketClient.getWebSocket().is_some() && startInstant.elapsed().as_secs() < 15 {
			webSocketClient.tick();
			std::thread::sleep(std::time::Duration::from_millis(20));
		}
		assert!(webSocketClient.getWebSocket().is_none());
		assert!(!webSocketClient.isConnected());
		webSocketClient.disconnect();
	}
}
