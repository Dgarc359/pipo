use core::fmt;
use std::marker::PhantomData;

use axum::{
    body::Body,
    http::{header, HeaderValue, Request, StatusCode, Uri},
    response::Response,
    routing::get,
    Router,
};
use bytes::BytesMut;
use ruma::api::{
    client::error::{ErrorBody, ErrorKind},
    OutgoingResponse,
};
use tower_http::validate_request::{ValidateRequest, ValidateRequestHeaderLayer};

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

struct Http {
    app: Router,
}

impl Http {
    fn new() -> Self {
        Self { app: Router::new() }
    }
    pub fn add_matrix_route(&mut self, hs_token: &str) {
        self.app = self
            .app
            .clone()
            .route("/_matrix/", get(|| async {}).fallback(unsupported_method)) // TODO: request method
            .fallback(unknown_route)
            .route_layer(ValidateRequestHeaderLayer::custom(MatrixBearer::new(
                hs_token,
            )));
    }
    // pub async fn serve(&self, listener: TcpListener) {
    //     axum::serve(listener, self.app).await.unwrap();
    // }
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
    use axum::{body::HttpBody, response::Json};
    use serde_json::{json, Value};
    use tower_service::Service;

    use super::*;

    const TEST_TOKEN: &'static str = "unit-test";

    async fn test_response(hs_token: &str, request: Request<Body>, expected: Response) {
        let mut http = Http::new();
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
}