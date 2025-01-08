use std::io::{Cursor, ErrorKind};
use axum::extract::{Multipart, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use har::{Error, Har};
use crate::api::{ApiResponse, AppError};
use crate::case::model::TestCase;
use crate::har_resolver::{build_test_case, filter_entries};
use crate::persistence::repo::{QueryResult, Repository};

pub async fn get_test_case(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> Result<ApiResponse<TestCase>, AppError> {
    let result = repository.test_cases().get("eren".to_string(), id).await;
    ApiResponse::from_option(result)
}

pub async fn list_test_cases(
    State(repository): State<Repository>,
) -> Result<ApiResponse<QueryResult<TestCase>>, AppError> {
    let result = repository.test_cases().list("eren".to_string(), None).await;
    ApiResponse::from(result)
}

pub async fn upload_test_case(
    State(repository): State<Repository>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut provided_har: Option<Har> = None;
    let mut provided_name: String = "".to_string();
    let mut provided_description: String = "".to_string();
    let mut provided_excluded_path_parts: Vec<String> = vec![];
    let mut provided_auth_providers: Vec<String> = vec![];
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        match name.as_str() {
            "name" => {
                provided_name = field.text().await.unwrap();
            }
            "description" => {
                provided_description = field.text().await.unwrap();
            }
            "auth_providers" => {
                provided_auth_providers = serde_json::from_str(field.text().await.unwrap().as_str()).unwrap();
            }
            "excluded_paths" => {
                provided_excluded_path_parts = field
                    .text()
                    .await
                    .unwrap()
                    .split(",")
                    .map(|s| s.to_string())
                    .collect();
            }
            "file" => {
                let data = field.bytes().await.unwrap();
                provided_har = Some(har::from_reader(Cursor::new(data)).unwrap());
            }
            _ => {}
        }
    }

    match provided_har {
        Some(har) => {
            build_test_case(
                &repository,
                &har.log,
                &"eren".to_string(),
                &provided_name,
                &provided_description,
                provided_excluded_path_parts.clone(),
                provided_auth_providers.clone(),
            )
                .await;
        }
        None => {}
    }
}

pub async fn filter_paths(mut multipart: Multipart) -> Result<ApiResponse<Vec<String>>, AppError> {
    let mut provided_har: Result<Har, Error> = Err(Error::Io(std::io::Error::new(
        ErrorKind::Other,
        "No Har found",
    )));
    let mut provided_excluded_path_parts: Vec<String> = vec![];
    while let Some(mut field) = multipart.next_field().await.unwrap() {
        let name = field.name().unwrap().to_string();
        match name.as_str() {
            "excluded_paths" => {
                provided_excluded_path_parts = field
                    .text()
                    .await
                    .unwrap()
                    .split(",")
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
            "file" => {
                let data = field.bytes().await.unwrap();
                provided_har = har::from_reader(Cursor::new(data))
            }
            _ => {}
        }
    }

    match provided_har {
        Ok(har) => {
            let urls: Vec<String> = filter_entries(provided_excluded_path_parts, &har.log)
                .iter()
                .map(|entry| &entry.request.url)
                .cloned()
                .collect();
            ApiResponse::from(Ok(urls))
        }
        Err(err) => Err(AppError::Processing(err.to_string())),
    }
}

pub async fn delete_test_case(
    Path(id): Path<String>,
    State(repository): State<Repository>,
) -> impl IntoResponse {
    repository
        .test_cases()
        .delete(&"eren".to_string(), &id)
        .await;
    StatusCode::NO_CONTENT
}