use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::str::FromStr;
use tracing::log::info;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct ReqParam {
    pub key: String,
    pub value: String,
}

impl ReqParam {
    pub fn new(key: String, value: String) -> Self {
        ReqParam { key, value }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ReqBody {
    pub value: Option<Value>,
}

impl ReqBody {

    pub fn empty() -> Self {
        ReqBody { value: None }
    }
    pub fn new(value: Value) -> Self {
        Self {
            value: Some(value),
        }
    }
}

pub struct Endpoint {
    pub method: HttpMethod,
    pub path: String,
    pub query_params: Vec<ReqParam>,
    pub headers: Vec<ReqParam>,
}

impl Endpoint {
    pub fn new(
        method: HttpMethod,
        path: String,
        path_params: Vec<ReqParam>,
        query_params: Vec<ReqParam>,
        headers: Vec<ReqParam>,
    ) -> Endpoint {
        let mut raw_path = path;
        path_params.into_iter().for_each(|param| {
            raw_path = raw_path.replace(&param.key, &param.value);
        });
        Endpoint {
            method,
            path: raw_path,
            query_params,
            headers,
        }
    }

    pub fn to_url(&self) -> String {
        if self.query_params.len() == 0 {
            self.path.to_string()
        } else {
            let query = self
                .query_params
                .iter()
                .map(|param| format!("{}={}", param.key, param.value))
                .collect::<Vec<String>>()
                .join("&");
            format!("{}?{}", self.path, query)
        }
    }
}

pub struct HttpRequest {
    pub endpoint: Endpoint,
    pub req_body: ReqBody,
    pub content_type: String,
}

impl HttpRequest {
    pub fn new(endpoint: Endpoint, req_body: ReqBody, content_type: String) -> HttpRequest {
        HttpRequest {
            endpoint,
            req_body,
            content_type,
        }
    }

    pub fn get_body(&self) -> Option<Value> {
        self.req_body.value.clone()
    }
}

pub struct ResBody<T> {
    pub value: T,
}

impl<T> ResBody<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

pub struct HttpResult<T> {
    pub res_body: ResBody<T>,
    pub status_code: u16,
}

impl<T> HttpResult<T> {
    pub fn new(res_body: ResBody<T>, status_code: u16) -> Self {
        Self {
            res_body,
            status_code,
        }
    }
}
#[derive(Clone)]
pub enum HttpError {
    Status(u16, StatusError),
    Io(String),
}

impl HttpError {
    pub fn get_message(&self) -> String {
        match self {
            HttpError::Status(_, status_err) => match status_err {
                StatusError::ClientError(msg) => msg.to_string(),
                StatusError::ServerError(mgs) => mgs.to_string(),
            },
            HttpError::Io(msg) => msg.to_string(),
        }
    }
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
pub enum StatusError {
    ClientError(String),
    ServerError(String),
}

#[derive(Clone)]
pub struct ApiClient {
    client: Client,
}

#[derive(Debug)]
pub enum HttpMethod {
    POST,
    GET,
    PUT,
    PATCH,
    DELETE,
}

impl ToString for HttpMethod {
    fn to_string(&self) -> String {
        match self {
            HttpMethod::POST => "POST".to_string(),
            HttpMethod::GET => "GET".to_string(),
            HttpMethod::PUT => "PUT".to_string(),
            HttpMethod::PATCH => "PATCH".to_string(),
            HttpMethod::DELETE => "DELETE".to_string(),
        }
    }
}

impl FromStr for HttpMethod {
    type Err = String; // Error type to return in case of invalid input

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "POST" => Ok(HttpMethod::POST),
            "GET" => Ok(HttpMethod::GET),
            "PUT" => Ok(HttpMethod::PUT),
            "PATCH" => Ok(HttpMethod::PATCH),
            "DELETE" => Ok(HttpMethod::DELETE),
            _ => Err(format!("Invalid HTTP method: {}", s)),
        }
    }
}

impl ApiClient {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }

    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResult<Value>, HttpError> {
        info!("will execute http request!");
        let req = self.build_reqwest(request);
        let result = req.send().await;
        match result {
            Ok(response) => {
                let status_code = response.status();
                info!("http request executed, status_code: {}", status_code);
                if status_code.is_success() {
                    let response_string = response.text().await.unwrap();
                    let parsed: Value = serde_json::from_str(&response_string).unwrap();
                    Ok(HttpResult::new(ResBody::new(parsed), status_code.as_u16()))
                } else if status_code.is_client_error() {
                    let text = response.text().await.unwrap();
                    info!("http request failed: {}", text);
                    Err(HttpError::Status(
                        status_code.as_u16(),
                        StatusError::ClientError(text),
                    ))
                } else {
                    Err(HttpError::Status(
                        status_code.as_u16(),
                        StatusError::ServerError(response.text().await.unwrap()),
                    ))
                }
            }
            Err(error) => {
                info!("http request failed: {}", error);
                Err(HttpError::Io(error.to_string()))
            }
        }
    }

    fn build_reqwest(&self, request: HttpRequest) -> RequestBuilder {
        let endpoint = request.endpoint;
        let req_body = request.req_body;
        let content_type = request.content_type.clone();
        let url_string = endpoint.to_url();
        info!("url: {}", url_string);
        info!("content type: {}", content_type);
        info!("method: {:?}", endpoint.method);
        let url = Url::parse(&*url_string).unwrap();
        let library_method = match &endpoint.method {
            HttpMethod::POST => Method::POST,
            HttpMethod::GET => Method::GET,
            HttpMethod::PUT => Method::PUT,
            HttpMethod::PATCH => Method::PATCH,
            HttpMethod::DELETE => Method::DELETE,
        };

        let mut headers = HeaderMap::new();
        endpoint.headers.iter().cloned().for_each(|header| {
            headers.insert(
                HeaderName::from_bytes(&header.key.as_bytes()).unwrap(),
                HeaderValue::from_bytes(&header.value.as_bytes()).unwrap(),
            );
        });

        let mut req = self.client.request(library_method, url).headers(headers);

        if let Some(body) = &req_body.value {
            info!("request body: {}", &body.to_string());
            if content_type.contains("application/x-www-form-urlencoded") {
                req = req.form(&body);
            } else {
                req = req.json(&body);
            }
        }
        req
    }
}

fn build_cookie_header(cookies: &Vec<ReqParam>) -> HeaderValue {
    let header_value = cookies
        .iter()
        .map(|cookie| format!("{}={}", cookie.key, cookie.value))
        .collect::<Vec<String>>()
        .join(";");
    HeaderValue::from_str(&header_value).unwrap()
}
