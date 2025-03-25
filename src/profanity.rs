use std::env;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest_middleware::ClientBuilder;
use reqwest_retry::{policies::ExponentialBackoff, RetryTransientMiddleware};
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct APIResponse {
    message: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ChatResponse {
    choices: Vec<Choice>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Choice {
    message: Message,
}

#[derive(Debug, Deserialize, Serialize)]
struct Message {
    content: String,
}

pub async fn check_profanity(content: String) -> Result<String, handle_errors::Error> {
    let api_key = env::var("API_KEY").unwrap();
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(3);
    let client = ClientBuilder::new(reqwest::Client::new())
        .with(RetryTransientMiddleware::new_with_policy(retry_policy))
        .build();

    // 构建请求头
    let mut headers = HeaderMap::new();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", api_key)).unwrap(),
    );

    // 构建请求体 - 使用AI模型进行敏感词检测
    let request_body = json!({
        "model": "glm-4-air",
        "messages": [
            {
                "role": "system",
                "content": "你是一个专业的敏感词检测系统。请严格检查用户输入的内容，\
                          将所有敏感词替换为*号。只返回处理后的文本，不要添加任何解释。",
            },
            {
                "role": "user",
                "content": format!("需要检查的内容如下：\n{}",content),
            }
        ],
        "temperature": 0.1  // 降低随机性，确保稳定输出
    });

    let res = client
        .post("https://open.bigmodel.cn/api/paas/v4/chat/completions")
        .headers(headers)
        .json(&request_body)
        .send()
        .await
        .map_err(|e| handle_errors::Error::MiddlewareReqwestAPIError(e))?;

    // 处理HTTP错误
    if !res.status().is_success() {
        if res.status().is_client_error() {
            let err = transform_error(res).await;
            return Err(handle_errors::Error::ClientError(err));
        } else {
            let err = transform_error(res).await;
            return Err(handle_errors::Error::ServerError(err));
        }
    }

    // 解析响应
    match res.json::<ChatResponse>().await {
        Ok(res) => {
            if let Some(first_choice) = res.choices.first() {
                Ok(first_choice.message.content.trim().to_string())
            } else {
                Err(handle_errors::Error::ServerError(handle_errors::APILayerError {
                    status: 500,
                    message: "No response from AI model".to_string(),
                }))
            }
        }
        Err(e) => Err(handle_errors::Error::ReqwestAPIError(e)),
    }
}

async fn transform_error(res: reqwest::Response) -> handle_errors::APILayerError {
    handle_errors::APILayerError {
        status: res.status().as_u16(),
        message: res.json::<APIResponse>().await.unwrap_or_else(|_| APIResponse {
            message: "Unknown error".to_string(),
        }).message,
    }
}