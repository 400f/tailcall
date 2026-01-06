use std::collections::HashMap;

use anyhow::Result;
use async_graphql::ServerError;
use http::{Request, Response};
use serde::de::DeserializeOwned;
use url::Url;

use crate::core::app_context::AppContext;
use crate::core::async_graphql_hyper::{Body, GraphQLRequestLike, GraphQLResponse};
use crate::core::blueprint::Blueprint;
use crate::core::config::reader::ConfigReader;
use crate::core::rest::EndpointSet;
use crate::core::runtime::TargetRuntime;

pub async fn create_app_ctx<T: DeserializeOwned + GraphQLRequestLike, B>(
    req: &Request<B>,
    runtime: TargetRuntime,
    enable_fs: bool,
) -> Result<Result<AppContext, Response<Body>>> {
    let config_url = req
        .uri()
        .query()
        .and_then(|x| serde_qs::from_str::<HashMap<String, String>>(x).ok())
        .and_then(|x| x.get("config").cloned());

    let config_url = if let Some(config_url) = config_url {
        config_url
    } else {
        let mut response = async_graphql::Response::default();
        let server_error = ServerError::new("No Config URL specified", None);
        response.errors = vec![server_error];
        return Ok(Err(GraphQLResponse::from(response).into_response()?));
    };

    if !enable_fs && Url::parse(&config_url).is_err() {
        let mut response = async_graphql::Response::default();
        let server_error = ServerError::new("Invalid Config URL specified", None);
        response.errors = vec![server_error];
        return Ok(Err(GraphQLResponse::from(response).into_response()?));
    }

    let reader = ConfigReader::init(runtime.clone());
    let config = match reader.read(config_url).await {
        Ok(config) => config,
        Err(e) => {
            let mut response = async_graphql::Response::default();
            let server_error = ServerError::new(format!("Failed to read config: {}", e), None);
            response.errors = vec![server_error];
            return Ok(Err(GraphQLResponse::from(response).into_response()?));
        }
    };

    let blueprint = match Blueprint::try_from(&config) {
        Ok(blueprint) => blueprint,
        Err(e) => {
            let mut response = async_graphql::Response::default();
            let server_error = ServerError::new(format!("{}", e), None);
            response.errors = vec![server_error];
            return Ok(Err(GraphQLResponse::from(response).into_response()?));
        }
    };

    Ok(Ok(AppContext::new(
        blueprint,
        runtime,
        EndpointSet::default(),
    )))
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "Needs refactoring for hyper 1.0"]
    async fn works_with_file() {
        // TODO: Refactor for hyper 1.0 - Incoming cannot be directly
        // constructed
    }
}
