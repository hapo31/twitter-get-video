extern crate base64;
extern crate envfile;

use base64::encode as base64_encode;
use core::panic;
use envfile::EnvFile;
use regex::Regex;
use reqwest::{header, Response};
use serde_json::{from_str, Value};
use std::{env, fs::create_dir, fs::File, io::Write, path::Path, process};
use urlencoding::encode;

#[tokio::main]
async fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() <= 1 {
        println!("Usage: ./get-media-twitter [tweet-url]");
        process::exit(1)
    }
    let tweet_url = &args[1];
    let re = Regex::new(r"https://twitter.com/(.*)/status/(\d+)").unwrap();
    let caps = re.captures(&tweet_url).unwrap();
    let author = &caps[1];
    let tweet_id = &caps[2];

    let env = match EnvFile::new(&Path::new(".env")) {
        Ok(v) => v,
        Err(_) => {
            panic!(".env not found in cwd.");
        }
    };

    let consumer_key = &env.store["CONSUMER_KEY"];
    let consumer_secret = &env.store["CONSUMER_SECRET"];

    let access_token_result = fetch_access_token(&consumer_key, &consumer_secret).await;

    let access_token: String = match access_token_result {
        Ok(v) => v["access_token"].as_str().unwrap().to_string(),
        Err(err) => panic!("{}", err),
    };

    let result = match fetch_tweet(&access_token, tweet_id).await {
        Ok(v) => from_str(&v.text().await.unwrap()).expect("JSON parse failed."),
        Err(e) => panic!("{}", e),
    };

    let video_url = extract_tweet_video_url(&result).await;

    match video_url {
        Ok(result) => {
            if let Some(url) = result {
                println!("{}", url);
                let path = format!("{}/{}.mp4", author, tweet_id);
                let filepath = Path::new(&path);
                fetch_video(&url, filepath).await.unwrap();
                println!("file saved [{}]", filepath.display());
            }
        }
        Err(err) => println!("{}", err),
    };
}

async fn fetch_access_token(
    consumer_key: &str,
    consumer_secret: &str,
) -> Result<Value, reqwest::Error> {
    let basic_auth = base64_encode(format!(
        "{}:{}",
        encode(consumer_key),
        encode(consumer_secret)
    ));

    let client = reqwest::Client::new();

    let body = "grant_type=client_credentials";

    let oauth_result = client
        .post("https://api.twitter.com/oauth2/token")
        .header(header::AUTHORIZATION, format!("Basic {}", basic_auth))
        .header(
            header::CONTENT_TYPE,
            "application/x-www-form-urlencoded;charset=UTF-8",
        )
        .header(header::CONTENT_LENGTH, body.len())
        .body(body)
        .send()
        .await;

    match oauth_result {
        Ok(v) => {
            let json_str = v.text().await.unwrap();
            Ok(from_str::<Value>(&json_str).expect("JSON parse failed."))
        }
        Err(err) => Err(err),
    }
}

async fn fetch_tweet(access_token: &str, tweet_id: &str) -> Result<Response, reqwest::Error> {
    let client = reqwest::Client::new();
    // let endpoint = format!("https://api.twitter.com/2/tweets/{}?expansions=author_id,referenced_tweets.id&media.fields=media_key,preview_image_url,url", tweet_id);
    let endpoint = format!(
        "https://api.twitter.com/1.1/statuses/show.json?id={}",
        tweet_id
    );
    client
        .get(&endpoint)
        .header(header::AUTHORIZATION, format!("Bearer {}", access_token))
        .send()
        .await
}

async fn extract_tweet_video_url(value: &Value) -> Result<Option<String>, reqwest::Error> {
    let variants = match value
        .get("extended_entities")
        .unwrap()
        .get("media")
        .unwrap()
        .get(0)
        .unwrap()
        .get("video_info")
        .unwrap()
        .get("variants")
    {
        Some(v) => v.as_array().unwrap(),
        None => return Ok(None),
    };

    let mut max_bitrate = 0;
    let mut max_bitrate_video_url = "";
    for variant in variants {
        if let Some(bitrate_value) = variant.get("bitrate") {
            if let Some(bitrate) = bitrate_value.as_u64() {
                if bitrate > max_bitrate {
                    max_bitrate = bitrate;
                    if let Some(url) = variant.get("url") {
                        max_bitrate_video_url = url.as_str().unwrap();
                    }
                }
            }
        }
    }

    Ok(Some(max_bitrate_video_url.to_string()))
}

async fn fetch_video(url: &str, filepath: &Path) -> Result<usize, std::io::Error> {
    let client = reqwest::Client::new();

    let result = client
        .get(url)
        .send()
        .await
        .expect(&format!("failed to fetch {}", url));

    let bytes = result
        .bytes()
        .await
        .expect(&format!("broken video file. {}", url));

    let parent = filepath.parent().unwrap();
    if !parent.exists() {
        create_dir(parent).expect("failed to create dir.");
    }

    let mut file = File::create(filepath)?;

    file.write(&bytes)
}
