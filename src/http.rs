use core::fmt;
use regex::{escape, Regex};
use std::{collections::HashMap, marker::PhantomData, sync::Arc};
use tokio::sync::broadcast::Sender;

use crate::{Attachment, Message};
use axum::{
    body::Body,
    extract::{Extension, Path, Query, Request as RequestExtractor, State},
    http::{header, HeaderValue, Request, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Router,
};
use bytes::{Bytes, BytesMut};
use ruma::api::{
    appservice,
    appservice::event::push_events::v1::Request as RumaPushEventRequest,
    appservice::ping::send_ping::v1::Request as RumaPingRequest,
    appservice::query::query_room_alias::v1::Request as RumaQueryRoomAliasRequest,
    appservice::query::query_user_id::v1::Request as RumaQueryUserIdRequest,
    appservice::thirdparty::get_location_for_protocol::v1::Request as RumaGetThirdpartyLocationForProtocol,
    appservice::thirdparty::get_location_for_room_alias::v1::{
        Request as RumaGetLocationForRoomAliasRequest,
        Response as RumaGetLocationForRoomAliasResponse,
    },
    appservice::thirdparty::get_protocol::v1::Request as RumaGetProtocolRequest,
    appservice::thirdparty::get_user_for_protocol::v1::Request as RumaGetUserForProtocolRequest,
    appservice::thirdparty::get_user_for_user_id::v1::Request as RumaGetThirdpartyUserForUIDRequest,
    client::error::{ErrorBody, ErrorKind},
    IncomingRequest, IncomingResponse, OutgoingRequest, OutgoingResponse,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::validate_request::{ValidateRequest, ValidateRequestHeaderLayer};

const MATRIX_HANDLERS_RELEASED: bool = true;
enum MatrixErrorCode {
    MForbidden,
}

struct MatrixError {
    errcode: MatrixErrorCode,
    error: String,
}

struct MatrixBearer {
    header_value: HeaderValue,
    _ty: PhantomData<fn() -> Body>,
}

impl MatrixBearer {
    fn new(token: &str) -> Self {
        Self {
            header_value: format!("Bearer {}", token)
                .parse()
                .expect("token is not a valid header value"),
            _ty: PhantomData,
        }
    }
}

impl Clone for MatrixBearer {
    fn clone(&self) -> Self {
        Self {
            header_value: self.header_value.clone(),
            _ty: PhantomData,
        }
    }
}

impl fmt::Debug for MatrixBearer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bearer")
            .field("header_value", &self.header_value)
            .finish()
    }
}

impl<B> ValidateRequest<B> for MatrixBearer {
    type ResponseBody = Body;

    fn validate(&mut self, request: &mut Request<B>) -> Result<(), Response<Self::ResponseBody>> {
        match request.headers().get(header::AUTHORIZATION) {
            Some(actual) if actual == self.header_value => Ok(()),
            _ => Err(ErrorBody::Standard {
                kind: ErrorKind::Forbidden,
                message: "Bad token supplied".to_string(),
            }
            .into_error(StatusCode::FORBIDDEN)
            .try_into_http_response::<BytesMut>()
            .expect("failed to construct response")
            .map(|b| b.freeze().into())
            .into()),
        }
    }
}

struct AppState {
    pub bus: HashMap<String, Sender<Message>>,
}

pub struct Http {
    pub app: Router,
    // TODO: this can be a shared reference in a few different places.
    pub channels: HashMap<String, Sender<Message>>,
}

impl Http {
    pub fn new(channels: HashMap<String, Sender<Message>>) -> Self {
        Self {
            app: Router::new(),
            channels,
        }
    }
    pub fn add_matrix_route(&mut self, hs_token: &str) {
        let shared = Arc::new(AppState {
            bus: self.channels.clone(),
        });
        self.app = self
            .app
            .clone()
            .route("/", get(handle_root_get).fallback(unsupported_method))
            .route("/_matrix/", get(|| async {}).fallback(unsupported_method)) // TODO: request method
            .route(
                "/_matrix/app/v1/users/:userId",
                get(get_user).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/transactions/:txnId",
                put(put_transaction)
                    .layer(Extension(shared.clone()))
                    .fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/rooms/:room",
                get(get_room).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/thirdparty/protocol/:protocol",
                get(get_thirdparty_protocol).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/ping",
                post(post_ping).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/thirdparty/location",
                get(get_thirdparty_location).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/thirdparty/location/:protocol",
                get(get_location_protocol).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/thirdparty/user",
                get(get_thirdparty_user).fallback(unsupported_method),
            )
            .route(
                "/_matrix/app/v1/thirdparty/user/:protocol",
                get(get_thirdparty_user_protocol).fallback(unsupported_method),
            )
            .fallback(unknown_route)
            .layer(Extension(shared))
            .route_layer(ValidateRequestHeaderLayer::custom(MatrixBearer::new(
                hs_token,
            )));
    }
}

async fn handle_root_get() -> Response {
    dbg!("root path called");
    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(
            json!({
                "hello": "world"
            })
            .to_string(),
        ))
        .unwrap();

    return response;
}

async fn handle_get_thirdparty_user_protocol(request: RumaGetUserForProtocolRequest) {
    todo!("handle get thirdparty user protocol");
}

async fn get_thirdparty_user_protocol(
    Path(protocol): Path<String>,
    request: RequestExtractor,
) -> Response {
    dbg!("gtp");
    let req: RumaGetUserForProtocolRequest = RumaGetUserForProtocolRequest::try_from_http_request(
        into_bytes_request(request).await,
        &vec![protocol],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_thirdparty_user_protocol(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_get_thirdparty_user(request: RumaGetThirdpartyUserForUIDRequest) {
    todo!("handle tp get user")
}

#[derive(Deserialize)]
struct GetThirdpartyUser {
    userid: String,
}

async fn get_thirdparty_user(
    userid: Query<GetThirdpartyUser>,
    request: RequestExtractor,
) -> Response {
    dbg!("gtpu");
    let req: RumaGetThirdpartyUserForUIDRequest =
        RumaGetThirdpartyUserForUIDRequest::try_from_http_request(
            into_bytes_request(request).await,
            &vec![userid.userid.to_owned()],
        )
        .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_thirdparty_user(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_get_location_protocol(request: RumaGetThirdpartyLocationForProtocol) {
    todo!("handle get location protocol")
}

async fn get_location_protocol(
    Path(protocol): Path<String>,
    request: RequestExtractor,
) -> Response {
    dbg!("gtlp");
    let req = RumaGetThirdpartyLocationForProtocol::try_from_http_request(
        into_bytes_request(request).await,
        &vec![protocol],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_location_protocol(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}
trait IntoAxumResponse {
    fn into_axum_res(&self) -> Response;
}

impl IntoAxumResponse for RumaGetLocationForRoomAliasResponse {
    fn into_axum_res(&self) -> Response {
        todo!("into axum response");
    }
}

async fn handle_get_thirdparty_location(
    request: RumaGetLocationForRoomAliasRequest,
) -> RumaGetLocationForRoomAliasResponse {
    todo!("handle get thirdparty location")
}

async fn get_thirdparty_location(request: RequestExtractor) -> Response {
    dbg!("gtpl");
    let req = RumaGetLocationForRoomAliasRequest::try_from_http_request::<_, &'static str>(
        into_bytes_request(request).await,
        &[],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        let res = handle_get_thirdparty_location(req).await;

        let http_res = res.try_into_http_response().unwrap();

        let (parts, body): (_, Vec<u8>) = http_res.into_parts();
        let body_v = Body::new(json!(&body).to_string());

        let response = Response::from_parts(parts, body_v);

        return response;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_post_ping(request: RumaPingRequest) {
    todo!("handle ping request")
}

async fn post_ping(request: RequestExtractor) -> Response {
    dbg!("postping");
    let req: RumaPingRequest = RumaPingRequest::try_from_http_request::<_, &'static str>(
        into_bytes_request(request).await,
        &[],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_post_ping(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_get_thirdparty_protocol(request: RumaGetProtocolRequest) {
    todo!("handling get thirdparty protocol")
}

async fn get_thirdparty_protocol(
    Path(protocol): Path<String>,
    request: RequestExtractor,
) -> Response {
    dbg!("gtptcl");
    let req: RumaGetProtocolRequest = RumaGetProtocolRequest::try_from_http_request(
        into_bytes_request(request).await,
        &vec![protocol],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_thirdparty_protocol(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_get_room(request: RumaQueryRoomAliasRequest) {
    todo!("handle getting room")
}

async fn get_room(Path(room): Path<String>, request: RequestExtractor) -> Response {
    dbg!("gtroom");
    let req: RumaQueryRoomAliasRequest = RumaQueryRoomAliasRequest::try_from_http_request(
        into_bytes_request(request).await,
        &vec![room],
    )
    .expect("Error Parsing get room");

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_room(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn into_bytes_request(request: Request<Body>) -> axum::http::Request<Bytes> {
    let (parts, body) = request.into_parts();
    let body = axum::body::to_bytes(body, usize::MAX)
        .await
        .expect("Error casting request body to bytes");

    let request = axum::extract::Request::from_parts(parts.into(), body);

    request
}

async fn handle_put_transaction(request: RumaPushEventRequest, state: Arc<AppState>) {
    #[derive(Debug, Deserialize)]
    struct MaybeStateEvent {
        state_key: Option<String>,
    }

    #[derive(Debug, Deserialize)]
    struct UndecidedNonStateEvent {
        pub content: serde_json::Value,
        pub room_id: String,
        pub sender: String,
    }

    #[derive(Debug, Deserialize)]
    struct RoomInviteEvent {
        age: i64,
        content: ruma::events::room::member::RoomMemberEventContent,
    }

    dbg!("bus: {:#?}", &state.bus);

    for event in request.events.iter() {
        dbg!("event! {:#?}", event);

        let maybe_state_event: MaybeStateEvent =
            serde_json::from_str(event.clone().into_json().get()).unwrap();

        // matrix api spec on handling transactions recommends checking for state_key
        // to determine if txn is state type or otherwise...
        // https://spec.matrix.org/v1.10/application-service-api/#put_matrixappv1transactionstxnid
        if maybe_state_event.state_key.is_some() {
            dbg!("I got a state event in transactions.. handle this");
            continue;
        }

        let undecided_event =
            serde_json::from_str::<UndecidedNonStateEvent>(event.clone().into_json().get())
                .unwrap();

        let maybe_text_event = serde_json::from_value::<
            ruma::events::room::message::TextMessageEventContent,
        >(undecided_event.content);

        match maybe_text_event {
            Ok(text) => {
                dbg!("got event text {:?}", &text.body);
                // send Message to other buses
                // get room id and match room id to regex
                // send text to appropriate channel
                for (channel_regex, channel) in &state.bus {
                    let re = Regex::new(&channel_regex).unwrap();
                    if re.is_match(&undecided_event.room_id) {
                        println!("sending message");
                        let res = channel.send(Message::Text {
                            sender: 1223,
                            pipo_id: 12345,
                            // TODO: send to other transports
                            transport: "IRC".to_string(),
                            username: undecided_event.sender.clone(),
                            avatar_url: None,
                            thread: None,
                            message: Some(text.body.clone()),
                            attachments: None,
                            is_edit: false,
                            irc_flag: true,
                        });

                        match res {
                            Ok(r) => {
                                println!("successfully sent, response {:#?}", r);
                            }
                            Err(e) => {
                                eprintln!("issue sending to channels, {:#?}", e);
                            }
                        }
                    } else {
                        println!("match not found");
                    }
                }
            }
            Err(e) => {
                dbg!("some error getting text event {:#?}", e);
            }
        }

        /*
        let deserded: RoomInviteEvent = serde_json::from_str(event.clone().into_json().get()).unwrap();
        dbg!("did we deserde? {:#?}", &deserded);
        */

        /*

        match deserded.content.membership {
            ruma::events::room::member::MembershipState::Invite => {
                dbg!("got an invite!!");
            },
            _ => { dbg!("we havent implemented this"); }
        }
        */
    }
}

async fn put_transaction(
    Extension(state): Extension<Arc<AppState>>,
    Path(tid): Path<String>,
    request: RequestExtractor,
) -> Response {
    dbg!("puttxn");
    dbg!("payload: {:#?}", &request);
    let req: RumaPushEventRequest =
        RumaPushEventRequest::try_from_http_request(into_bytes_request(request).await, &vec![tid])
            .unwrap();

    dbg!("serdededredeed {:#?}", &req);

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_put_transaction(req, state).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn handle_get_user(request: RumaQueryUserIdRequest) {
    todo!("handle getting user")
}

async fn get_user(Path(user_id): Path<String>, request: RequestExtractor) -> Response {
    dbg!("gtusr");
    let req: RumaQueryUserIdRequest = RumaQueryUserIdRequest::try_from_http_request(
        into_bytes_request(request).await,
        &vec![user_id],
    )
    .unwrap();

    if MATRIX_HANDLERS_RELEASED {
        // do whatever it takes.
        handle_get_user(req).await;
    };

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(Body::new(json!({}).to_string()))
        .unwrap();

    response
}

async fn unknown_route(uri: Uri) -> Response {
    ErrorBody::Standard {
        kind: ErrorKind::Unrecognized,
        message: "Unknown endpoint".to_string(),
    }
    .into_error(StatusCode::NOT_FOUND)
    .try_into_http_response::<BytesMut>()
    .expect("failed to construct response")
    .map(|b| b.freeze().into())
    .into()
}

async fn unsupported_method(uri: Uri) -> Response {
    ErrorBody::Standard {
        kind: ErrorKind::Unrecognized,
        message: "Unsupported method".to_string(),
    }
    .into_error(StatusCode::METHOD_NOT_ALLOWED)
    .try_into_http_response::<BytesMut>()
    .expect("failed to construct response")
    .map(|b| b.freeze().into())
    .into()
}

#[cfg(test)]
mod tests {
    use axum::{body::HttpBody, extract::FromRequest, response::Json};
    use ruma::{api::MatrixVersion, room_alias_id, room_id};
    use serde_json::{json, Value};
    use tower_service::Service;

    use super::*;

    const TEST_TOKEN: &'static str = "unit-test";

    async fn test_response(hs_token: &str, request: Request<Body>, expected: Response) {
        let mut http = Http::new(HashMap::new());
        http.add_matrix_route(hs_token);
        // TODO(nemo): Decide if using tower_service::oneshot() is better
        // than call().
        let res = http.app.call(request).await.unwrap();
        assert_eq!(res.status(), expected.status());
        let res_body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        let exp_body = axum::body::to_bytes(expected.into_body(), usize::MAX)
            .await
            .unwrap();
        assert_eq!(res_body, exp_body);
    }

    // TODO(nemo): Clean up test
    #[tokio::test]
    async fn matrix_handle_invalid_token() {
        let hs_token = "abcd";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/")
            .header(header::AUTHORIZATION, "Bearer dcba")
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::FORBIDDEN)
            .body(Body::new(
                json!({ "errcode": "M_FORBIDDEN", "error": "Bad token supplied" }).to_string(),
            ))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_valid_token() {
        let hs_token = "abcd";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_unknown_endpoint() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/unknown/")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::new(
                json!({ "errcode": "M_UNRECOGNIZED", "error": "Unknown endpoint" }).to_string(),
            ))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_known_endpoint() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_unsupported_method() {
        let hs_token = TEST_TOKEN;
        let request = Request::builder()
            .method("POST")
            .uri("/_matrix/")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(Body::new(
                json!({ "errcode": "M_UNRECOGNIZED", "error": "Unsupported method" }).to_string(),
            ))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_user() {
        let hs_token = "test_handle_get_users";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/users/@example:example.org")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_put_transactions() {
        let hs_token = "test_handle_unknown_endpoint";

        let r = Request::builder()
            .method("PUT")
            .uri("/_matrix/app/v1/transactions/1")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::new(
                json!({"events": vec![json!({})], "txn_id": "id".to_owned()}).to_string(),
            ))
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, r, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_room_alias() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/rooms/%23room:alias.com")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_thirdparty_protocol() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/thirdparty/protocol/chosen-protocol")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_post_ping() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("POST")
            .uri("/_matrix/app/v1/ping")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::new(
                json!({"transaction_id": "mautrix-go_1683636478256400935_123" }).to_string(),
            ))
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_thirdparty_location() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/thirdparty/location?alias=%23room:example.com")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_thirdparty_location_protocol() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/thirdparty/location/chosen-protocol")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_thirdparty_user() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/thirdparty/user?userid=@user:example.com")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }

    #[tokio::test]
    async fn matrix_handle_get_thirdparty_user_protocol() {
        let hs_token = "test_handle_unknown_endpoint";
        let request = Request::builder()
            .method("GET")
            .uri("/_matrix/app/v1/thirdparty/user/chosen-user-protocol")
            .header(header::AUTHORIZATION, format!("Bearer {hs_token}"))
            .body(Body::empty())
            .unwrap();
        let expected = Response::builder()
            .status(StatusCode::OK)
            .body(Body::new(json!({}).to_string()))
            .unwrap();
        test_response(hs_token, request, expected).await;
    }
}
