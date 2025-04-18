use reqwest::Client;
use serde::Deserialize;

use std::{collections::VecDeque, sync::{Arc, Mutex, OnceLock}, time::Instant};
use dashmap::DashMap;
use futures::stream::{FuturesUnordered, StreamExt};
use tokio::{sync::Semaphore, task, time::{sleep, Duration}};
use colored::Colorize;

use crate::structures::*;





const NODE_SERVER_URL: &str = "http://localhost:3000";
const COMBINE_LOGS: bool = true;
const COMBINE_RETRIES: u64 = 10;
const COMBINE_TIMEOUT: u64 = 5 * 60;   // 5 minute timeout to local server
const RPS_TRACKER_WINDOW: u64 = 60;
pub const MAX_CONCURRENT_REQUESTS: usize = 150;


static CLIENT: OnceLock<Client> = OnceLock::new();

pub static REQUEST_STATS: OnceLock<Mutex<RequestStats>> = OnceLock::new();



#[derive(Debug)]
pub struct RequestStats {
    pub outgoing_requests: u32,
    pub responded_requests: u32,
    pub to_request: u32,
    pub start_time: Instant,
    pub rps_tracker: RpsTracker,
}
impl Default for RequestStats {
    fn default() -> Self {
        RequestStats {
            outgoing_requests: 0,
            responded_requests: 0,
            to_request: 0,
            start_time: Instant::now(),
            rps_tracker: RpsTracker::new(),
        }
    }
}






// Structure to match the JSON response from the Node server
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
                             NODE_SERVER_URL,
                             urlencoding::encode(first),
                             urlencoding::encode(second));


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
                    if COMBINE_LOGS {
                        println!("Rust: Failed to parse SUCCESS JSON: {}. Body was: {}", e, response_text);
                        continue;
                    };
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
    let num_to_str = variables.num_to_str.read().unwrap();
    let str_to_num: DashMap<String, u32> = num_to_str
                .iter()
                .enumerate()
                .map(|(i, str)| (str.clone(), i as u32))
                .collect();

    drop(num_to_str);

    // reset Request Stats
    let mut rs = REQUEST_STATS.get_or_init(|| Mutex::new(RequestStats::default())).lock().expect("lock poisoned");
    *rs = RequestStats::default();
    rs.to_request = variables.to_request_recipes.len() as u32;
    drop(rs);


    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_REQUESTS));
    let mut futures = FuturesUnordered::new();

    println!(
        "Processing {} recipes...",
        variables.to_request_recipes.len()
    );


    for entry in variables.to_request_recipes.iter() {
        let comb = *entry.key();

        let sem_clone = Arc::clone(&semaphore);

        futures.push(task::spawn(async move {
            // Wait for a permit *before* doing work or accessing shared data
            let _permit = sem_clone.acquire_owned().await.expect("Semaphore acquisition failed");

            let first; 
            let second;
            {
                let num_to_str = variables.num_to_str.read().unwrap();
                first = num_to_str[comb.0 as usize].clone();
                second = num_to_str[comb.1 as usize].clone();

                let mut rs = REQUEST_STATS.get().expect("REQUEST_STATS not initialized").lock().expect("lock poisoned");
                rs.outgoing_requests += 1;
            }

            let result_str = combine(&first, &second).await
                .map_or_else(|| String::from("Nothing"), |res| res.result);

            (comb, result_str)            
        }));
    }
    variables.to_request_recipes.clear();

            

    while let Some(task_result) = futures.next().await {
        let mut rs = REQUEST_STATS.get().expect("REQUEST_STATS not initialized.").lock().expect("lock poisoned");
        rs.responded_requests += 1;
        rs.rps_tracker.increment();
        drop(rs);

        match task_result {
            Ok((comb, result_str)) => {
                let mut num_to_str = variables.num_to_str.write().unwrap();
                let mut recipes_ing = variables.recipes_ing.write().unwrap();
                let mut neal_case_map = variables.neal_case_map.write().unwrap();

                // add recipe
                match str_to_num.get(&result_str) {
                    Some(num) => {
                        // result already exists
                        recipes_ing.insert(sort_recipe_tuple(comb), *num.value());
                    }
                    None => {
                        // result does not exist, add it
                    
                        let id = num_to_str.len() as u32;
                        num_to_str.push(result_str.clone());
                        str_to_num.insert(result_str.clone(), id);
                        recipes_ing.insert(sort_recipe_tuple(comb), id);

                        let neal_str = start_case_unicode(&result_str.clone());
                        // add neal_str
                        match str_to_num.get(&neal_str) {
                            Some(x) => {
                                // neal case version exists, link to it
                                neal_case_map.push(*x);
                            }
                            None => {
                                // neal case version does not exist, create it and link to it
                                let neal_id = num_to_str.len() as u32;
                                str_to_num.insert(neal_str.clone(), neal_id);
                                num_to_str.push(neal_str);
                
                                // links result_str -> neal_id
                                neal_case_map.push(neal_id);
                                // links neal_id -> neal_id
                                neal_case_map.push(neal_id);
                            }
                        }
                    }
                }
            },
            Err(join_err) => {
                eprintln!("Task panicked or was cancelled: {}", join_err);
                sleep(Duration::from_secs(60)).await;  // await 60s
            },
        }
    }
}





fn interval_message(start_time: Instant, rs: RequestStats) {
    println!("Request Time: {},  Requests: {}/{},  Current Outgoing Requests: {},  Requests/s: (Total: {}, Last 60s: {})",
        format!("{:?}", start_time.elapsed()).yellow(),

        rs.responded_requests.to_string().yellow(),
        (rs.to_request).to_string().yellow(),

        (rs.outgoing_requests - rs.responded_requests).to_string().yellow(),
        
        format!("{:.3}", rs.responded_requests as f64 / rs.start_time.elapsed().as_secs_f64()).yellow(),
        format!("{:.3}", rs.rps_tracker.get_rps()).yellow(),
    );
}






#[derive(Debug)]
pub struct RpsTracker {
    timestamps: Arc<Mutex<VecDeque<Instant>>>,
    window: Duration,
}


impl RpsTracker {
    fn new() -> Self {
        RpsTracker {
            timestamps: Arc::new(Mutex::new(VecDeque::new())),
            window: Duration::from_secs(RPS_TRACKER_WINDOW),
        }
    }

    fn increment(&self) {
        let mut timestamps = self.timestamps.lock().expect("Mutex poisoned");
        timestamps.push_back(Instant::now());
    }

    pub fn get_rps(&self) -> f64 {
        let mut timestamps = self.timestamps.lock().expect("Mutex poisoned");
        let now = Instant::now();

        // --- Trim old timestamps ---
        while let Some(oldest) = timestamps.front() {
            if now.duration_since(*oldest) > self.window {
                timestamps.pop_front(); // Remove it if it's too old
            } else {
                // The rest are within the window (since VecDeque is ordered)
                break;
            }
        }

        // --- Calculate RPS ---
        let count_in_window = timestamps.len();
        let window_secs = self.window.as_secs_f64();
        if window_secs > 0.0 {
            count_in_window as f64 / window_secs
        } else {
            69420.0
        }
    }
}