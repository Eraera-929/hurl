/*
 * hurl (https://hurl.dev)
 * Copyright (C) 2020 Orange
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *          http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 */
use std::collections::HashMap;
use std::time::Instant;

use crate::core::ast::*;
use crate::core::common::SourceInfo;
use crate::core::common::Value;
use crate::http;

use super::core::*;
use super::core::{Error, RunnerError};
use crate::format::logger::Logger;

/// Run an entry with the hurl http client
///
/// # Examples
///
/// ```
/// use hurl::http;
/// use hurl::runner;
///
/// // Create an http client
//// let client = http::client::Client::init(http::client::ClientOptions {
////        noproxy_hosts: vec![],
////        insecure: false,
////        redirect: http::client::Redirect::None,
////        http_proxy: None,
////        https_proxy: None,
////        all_proxy: None
////    });
/// ```
pub fn run(
    entry: Entry,
    http_client: &mut http::Client,
    entry_index: usize,
    variables: &mut HashMap<String, Value>,
    context_dir: String,
    logger: &Logger,
) -> EntryResult {
    let http_request = match entry.clone().request.eval(variables, context_dir.clone()) {
        Ok(r) => r,
        Err(error) => {
            return EntryResult {
                request: None,
                response: None,
                captures: vec![],
                asserts: vec![],
                errors: vec![error],

                time_in_ms: 0,
            };
        }
    };

    logger
        .verbose("------------------------------------------------------------------------------");
    logger.verbose(format!("executing entry {}", entry_index + 1).as_str());

    // Temporary - add cookie from request to the cookie store
    // should be set explicitly
    // url should be valid at the point
    // do not use cookie from request
    use url::Url;
    if let Ok(url) = Url::parse(http_request.url.as_str()) {
        for c in http_request.cookies.clone() {
            let cookie = http::Cookie {
                domain: url.host_str().unwrap().to_string(),
                include_subdomain: "FALSE".to_string(),
                path: "/".to_string(),
                https: "FALSE".to_string(),
                expires: "0".to_string(),
                name: c.name,
                value: c.value,
            };
            http_client.add_cookie(cookie);
        }
    }

    logger.verbose("");
    logger.verbose("Cookie store:");
    for cookie in http_client.get_cookie_storage() {
        logger.verbose(cookie.to_string().as_str());
    }
    logger.verbose("");
    log_request(logger, &http_request);

    let start = Instant::now();
    let http_response = match http_client.execute(&http_request, 0) {
        Ok(response) => response,
        Err(_) => {
            return EntryResult {
                request: Some(http_request.clone()),
                response: None,
                captures: vec![],
                asserts: vec![],
                errors: vec![Error {
                    source_info: SourceInfo {
                        start: entry.clone().request.url.source_info.start,
                        end: entry.clone().request.url.source_info.end,
                    },
                    inner: RunnerError::HttpConnection {
                        message: "".to_string(),
                        url: http_request.url,
                    },
                    assert: false,
                }],
                time_in_ms: 0,
            };
        }
    };

    let time_in_ms = start.elapsed().as_millis();
    logger.verbose(format!("Response Time: {}ms", time_in_ms).as_str());

    let captures = match entry.response.clone() {
        None => vec![],
        Some(response) => match response.eval_captures(http_response.clone(), variables) {
            Ok(captures) => captures,
            Err(e) => {
                return EntryResult {
                    request: Some(http_request),
                    response: Some(http_response),
                    captures: vec![],
                    asserts: vec![],
                    errors: vec![e],
                    time_in_ms,
                };
            }
        },
    };

    // update variables now!
    for capture_result in captures.clone() {
        variables.insert(capture_result.name, capture_result.value);
    }

    let asserts = match entry.response {
        None => vec![],
        Some(response) => response.eval_asserts(variables, http_response.clone(), context_dir),
    };
    let errors = asserts
        .iter()
        .filter_map(|assert| assert.clone().error())
        .map(
            |Error {
                 source_info, inner, ..
             }| Error {
                source_info,
                inner,
                assert: true,
            },
        )
        .collect();

    if !captures.is_empty() {
        logger.verbose("Captures");
        for capture in captures.clone() {
            logger.verbose(format!("{}: {}", capture.name, capture.value).as_str());
        }
    }

    logger.verbose("");

    EntryResult {
        request: Some(http_request),
        response: Some(http_response),
        captures,
        asserts,
        errors,
        time_in_ms,
    }
}

pub fn log_request(logger: &Logger, request: &http::Request) {
    logger.verbose("Request");
    logger.verbose(format!("{} {}", request.method, request.url).as_str());
    for header in request.headers.clone() {
        logger.verbose(header.to_string().as_str());
    }
    if !request.querystring.is_empty() {
        logger.verbose("[QueryStringParams]");
        for param in request.querystring.clone() {
            logger.verbose(param.to_string().as_str());
        }
    }
    if !request.form.is_empty() {
        logger.verbose("[FormParams]");
        for param in request.form.clone() {
            logger.verbose(param.to_string().as_str());
        }
    }
    if !request.multipart.is_empty() {
        logger.verbose("[MultipartFormData]");
        for param in request.multipart.clone() {
            logger.verbose(param.to_string().as_str());
        }
    }
    if !request.cookies.is_empty() {
        logger.verbose("[Cookies]");
        for cookie in request.cookies.clone() {
            logger.verbose(cookie.to_string().as_str());
        }
    }
    if let Some(s) = request.content_type.clone() {
        logger.verbose("");
        logger.verbose(format!("implicit content-type={}", s).as_str());
    }
    logger.verbose("");
}
