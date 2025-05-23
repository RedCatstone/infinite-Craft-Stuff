use reqwest::Client;
use serde::Deserialize;

use std::{sync::{Arc, Mutex, OnceLock}, time::Instant};
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::{sync::Semaphore, task, time::Duration};
use colored::Colorize;

use crate::structures::*;





const REQUEST_SERVER_URL: &str = "http://localhost:3000";
const COMBINE_RETRIES: u64 = 10;
const COMBINE_TIMEOUT: u64 = 5 * 60;   // 5 minute timeout to local server
pub const MAX_CONCURRENT_REQUESTS: usize = 150;
const COMBINE_INTERVAL_MESSAGE_SECS: u64 = 30;


static CLIENT: OnceLock<Client> = OnceLock::new();



#[derive(Debug, Clone)]
pub struct RequestStats {
    pub outgoing_requests: u32,
    pub responded_requests: u32,
    pub to_request: u32,
    pub start_time: Instant,
}






// Structure to match the JSON response from the Request server
#[derive(Deserialize, Debug)]
pub struct CombineResponse {
    pub result: String,
    pub emoji: String,
    #[serde(rename = "isNew")]
    pub is_new: bool,
}


pub async fn combine(first: &str, second: &str) -> Option<CombineResponse> {
    // Build URL with query parameters
    let request_url = format!("{}/?first={}&second={}",
        REQUEST_SERVER_URL,
        urlencoding::encode(first),
        urlencoding::encode(second)
    );


    let client = CLIENT.get_or_init(|| {
        match Client::builder()
            .timeout(Duration::from_secs(COMBINE_TIMEOUT))
            .build() {
            Ok(c) => c,
            Err(e) => { panic!("Rust: Failed to build HTTP client: {}", e); },
        }
    });


    for _retries in 0..COMBINE_RETRIES {

        // println!("Rust: Sending request to server: {}", request_url);
        let response = match client.get(&request_url).send().await {
            Ok(res) => { res },
            Err(e) => {
                println!("Error {}", e);
                continue;
            }
        };

        let status = response.status();
        let response_text = response.text().await.expect("could not get response.text()"); // Get body text
        // println!("Rust: Received status: {}", status);

        if status.is_success() {
            // Try parsing as the success response
            match serde_json::from_str::<CombineResponse>(&response_text) {
                Ok(data) => {
                    return Some(CombineResponse { result: data.result, emoji: data.emoji, is_new: data.is_new });
                }
                Err(e) => {
                    println!("Rust: Failed to parse SUCCESS JSON: {}. Body was: {}", e, response_text);
                    continue;
                },
            }
        } else {
            // println!("Rust: Request failed: {:?}", status);
            continue;
        }
    }

    None
}












pub async fn process_all_to_request_recipes() {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let mut str_to_num = get_str_to_num_map();

    let request_stats_arc = Arc::new(Mutex::new(RequestStats {
        to_request: variables.to_request_recipes.len() as u32,
        outgoing_requests: 0,
        responded_requests: 0,
        start_time: Instant::now(),
    }));


    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut futures = FuturesUnordered::new();


    let rs_clone_for_interval = request_stats_arc.clone();
    let interval_task = tokio::spawn(async move {
        let mut interval_timer = tokio::time::interval(Duration::from_secs(COMBINE_INTERVAL_MESSAGE_SECS));
        loop {
            interval_timer.tick().await;
            let rs_data = rs_clone_for_interval.lock().expect("Interval lock poisoned").clone();
            interval_message(rs_data);
        }
    });


    for entry in variables.to_request_recipes.iter() {
        let comb = *entry.key();

        let sem_clone = Arc::clone(&semaphore);

        let rs_clone_for_task = request_stats_arc.clone();

        futures.push(task::spawn(async move {
            // Wait for a permit *before* doing work or accessing shared data
            let _permit = sem_clone.acquire_owned().await.expect("Semaphore acquisition failed");

            let first_str;
            let second_str;
            {
                let num_to_str = variables.num_to_str.read().unwrap();
                first_str = num_to_str[comb.0 as usize].clone();
                second_str = num_to_str[comb.1 as usize].clone();

                let mut rs = rs_clone_for_task.lock().expect("Outgoing lock poisoned");
                rs.outgoing_requests += 1;
            }

            let result_str = combine(&first_str, &second_str).await
                .map_or_else(|| String::from("Nothing"), |res| res.result);

            (first_str, second_str, result_str)
        }));
    }
    variables.to_request_recipes.clear();

            

    while let Some(task_result) = futures.next().await {
        {
            let mut rs = request_stats_arc.lock().expect("rs lock poisoned");
            rs.responded_requests += 1;
        }
        match task_result {
            Ok((first_str, second_str, result_str)) => {
                variables_add_recipe(first_str, second_str, result_str, &mut str_to_num);
            },
            Err(join_err) => {
                eprintln!("Task panicked or was cancelled: {}", join_err);
            },
        }
    }

    let rs = request_stats_arc.lock().expect("Final lock poisoned").clone();
    interval_message(rs);
    interval_task.abort();
}





fn interval_message(rs: RequestStats) {
    println!("Requests: {}/{},  Time: {},  Current Outgoing: {},  Rps: {}",
        rs.responded_requests.to_string().green(),
        (rs.to_request).to_string().green(),

        format!("{:?}", rs.start_time.elapsed()).green(),

        (rs.outgoing_requests - rs.responded_requests).to_string().green(),
        
        format!("{:.3}", rs.responded_requests as f64 / rs.start_time.elapsed().as_secs_f64()).green(),
    );
}