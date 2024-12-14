use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::{Client, Method, RequestBuilder, Url};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt::Display;
use std::str::FromStr;

#[derive(Serialize, Deserialize, Clone)]
pub struct ReqParam {
    key: String,
    value: String,
}

impl ReqParam {
    pub fn new(key: String, value: String) -> Self {
        ReqParam { key, value }
    }
}

#[derive(Serialize, Deserialize)]
pub struct ReqBody {
    value: Value,
}

impl ReqBody {
    pub fn new(value: Value) -> Self {
        Self { value }
    }
}

pub struct Endpoint {
    method: HttpMethod,
    path: String,
    query_params: Vec<ReqParam>,
    headers: Vec<ReqParam>,
    cookies: Vec<ReqParam>,
}

impl Endpoint {
    pub fn new(
        method: HttpMethod,
        path: String,
        path_params: Vec<ReqParam>,
        query_params: Vec<ReqParam>,
        headers: Vec<ReqParam>,
        cookies: Vec<ReqParam>,
    ) -> Endpoint {
        let mut raw_path = path;
        path_params.into_iter().for_each(|(param)| {
            raw_path = raw_path.replace(&param.key, &param.value);
        });
        Endpoint {
            method,
            path: raw_path,
            query_params,
            headers,
            cookies,
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
            format!("{}?{}",self.path, query)
        }
    }
}

pub struct HttpRequest {
    endpoint: Endpoint,
    req_body: ReqBody,
    content_type: String,
}

impl HttpRequest {
    pub fn new(endpoint: Endpoint, req_body: ReqBody, content_type: String) -> HttpRequest {
        HttpRequest { endpoint, req_body, content_type }
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
    status_code: u16,
}

impl<T> HttpResult<T> {
    pub fn new(res_body: ResBody<T>, status_code: u16) -> Self {
        Self {
            res_body,
            status_code,
        }
    }
}
pub enum HttpError {
    Status(StatusError),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
enum StatusError {
    ClientError(String),
    ServerError(String),
}

pub struct ApiClient {
    client: Client,
    auth_header: (String, String),
}

#[derive(Debug)]
pub enum HttpMethod {
    POST,
    GET,
    PUT,
    PATCH,
    DELETE,
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
    pub fn new(auth_header: (String, String)) -> Self {
        Self {
            client: Client::new(),
            auth_header,
        }
    }

    pub async fn execute(&self, request: HttpRequest) -> Result<HttpResult<Value>, HttpError> {
        println!("will execute http request!");
        let req = self.build_reqwest(request);
        let result = req.send().await;
        match result {
            Ok(response) => {
                let status_code = response.status();
                println!("http request executed, status_code: {}", status_code);
                if status_code.is_success() {
                    let response_string = response.text().await.unwrap();
                    println!("http request executed, response_string: {}", response_string.clone());
                    let parsed: Value = serde_json::from_str(&response_string).unwrap();
                    println!("response json: {:?}", parsed);
                    Ok(HttpResult::new(ResBody::new(parsed), status_code.as_u16()))
                } else if status_code.is_client_error() {
                    let text = response.text().await.unwrap();
                    println!("http request failed: {}", text);
                    Err(HttpError::Status(StatusError::ClientError(text)))
                } else {
                    Err(HttpError::Status(StatusError::ServerError(
                        response.text().await.unwrap(),
                    )))
                }
            }
            Err(error) => {
                println!("http request failed: {}", error);
                Err(HttpError::Status(StatusError::ClientError(
                    error.to_string(),
                )))
            }
        }
    }

    fn build_reqwest(&self, request: HttpRequest) -> RequestBuilder {
        let endpoint = request.endpoint;
        let req_body = request.req_body;
        let content_type = request.content_type.clone();
        let url_string = endpoint.to_url();
        println!("url: {}", url_string);
        println!("content type: {}", content_type);
        println!("method: {:?}", endpoint.method);
        println!("request body: {}", &req_body.value.to_string());
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
                HeaderName::try_from(header.key.replace(":", "")).unwrap(),
                HeaderValue::from_str(header.value.clone().as_str()).unwrap(),
            );
        });
        headers.insert("cookie", HeaderValue::from_static("ajs_anonymous_id=%22c39d9e3c-eb77-43b4-a4b8-221516f1fc09%22; OPSGENIE_ACTIVE_CUSTOMER=tester22; OPSGENIE_SSO_COOKIE=47ec193b-6bff-485a-b8be-e1981b931474#da0ebb76-0d24-41c1-9e04-395f0a274247; intercom-device-id-pd3wpn56=b43b3e9e-654e-49fb-bcbc-cb330b803584; __stripe_mid=c2057271-46bc-4d04-93d3-0ecee3d70eed9d4f34; csrf-token=fb637d79-decb-43a6-ad62-04920715a71d; JSESSIONID=WbS3fKejDUogHRL1wM8luYW28lpw50q1IRXpRUDB; cloud.session.token=eyJraWQiOiJzZXNzaW9uLXNlcnZpY2UvcHJvZC0xNTkyODU4Mzk0IiwiYWxnIjoiUlMyNTYifQ.eyJhc3NvY2lhdGlvbnMiOltdLCJzdWIiOiI3MTIwMjA6OWEwMDc4YjUtNDFkOC00ZTQ0LWEwOWYtYzljNmI5NTc5NzEzIiwiZW1haWxEb21haW4iOiJraW5kb21kLmNvbSIsImltcGVyc29uYXRpb24iOltdLCJjcmVhdGVkIjoxNzMyNzkyMjIxLCJyZWZyZXNoVGltZW91dCI6MTczMzg1NjYwNSwidmVyaWZpZWQiOnRydWUsImlzcyI6InNlc3Npb24tc2VydmljZSIsInNlc3Npb25JZCI6IjIwNmJjZDRiLWY5YTUtNDMxNC04ZDg0LTc3YzgyODYyYzVkMCIsInN0ZXBVcHMiOltdLCJhdWQiOiJhdGxhc3NpYW4iLCJuYmYiOjE3MzM4NTYwMDUsImV4cCI6MTczNjQ0ODAwNSwiaWF0IjoxNzMzODU2MDA1LCJlbWFpbCI6InBpdGFjaXA0NzVAa2luZG9tZC5jb20iLCJqdGkiOiIyMDZiY2Q0Yi1mOWE1LTQzMTQtOGQ4NC03N2M4Mjg2MmM1ZDAifQ.Jf7cTt_n46iTRl0_ffHyfwLigZGOCwfFzbtuICt5Kxp3F86sOHfiKVlY-1pGqADooyWmZiw01dd21ByhWtObusmu0ogMBPATRuHxS0cviI0M0F-VVtKMHUSY-tgHneO4hixfs5qtdUwouAghXawwrFQF_0vhBAFCH0N6g94KGJ35d8Gz1kyMrIL9sx5OvAwkAiy7gE91khdhKuiQMkhrcvX6PPQFSzygjSLfT7ZALVsIDWFkJSnOc9XywqySzJNQEAneD_c1FvJLEmiN7JyIpY6mnFK8BZt4EtkHHTyygItJw50L2ECl3nXJ4hsQB0TwD6gCqa7_vOXs8bMqXMxmRw"));
        headers.insert("csrf-token", HeaderValue::from_static("fb637d79-decb-43a6-ad62-04920715a71d"));
        //headers.insert("cookie", build_cookie_header(&endpoint.cookies));

        let mut req = self.client.request(library_method, url).headers(headers);

        if !req_body.value.is_null() {
            if content_type.contains("application/x-www-form-urlencoded") {
                req = req.form(&req_body.value);
            } else {
                req = req.json(&req_body.value);
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
