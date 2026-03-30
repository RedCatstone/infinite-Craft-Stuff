use num_format::ToFormattedString;
use serde::Deserialize;

use std::{sync::{Arc, Mutex, OnceLock}, time::Instant};
use futures::stream::StreamExt;
use tokio::{task, time::Duration};
use colored::Colorize;

use crate::structures::RecipesState;





const REQUEST_SERVER_URL: &str = "http://localhost:3000";
const COMBINE_RETRIES: u64 = 50;

/// the timeout from rust to the local:3000 server
const COMBINE_TIMEOUT: Duration = Duration::from_mins(5);

/// amount of outgoing requests at each time from rust to the local:3000 server
/// set to 150 by default to make sure that its a constant stream of requests. (you can modify it to something larger)
const MAX_CONCURRENT_REQUESTS: usize = 150;
const COMBINE_INTERVAL_MESSAGE_SECS: Duration = Duration::from_mins(1);


static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();


#[derive(Debug, Clone)]
pub struct RequestStats {
    pub outgoing_requests: usize,
    pub responded_requests: usize,
    pub to_request: usize,
    pub start_time: Instant,
    pub name: String,
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
        match reqwest::Client::builder().timeout(COMBINE_TIMEOUT).build() {
            Ok(c) => c,
            Err(e) => { panic!("Failed to build HTTP client: {e}"); },
        }
    });

    let mut attempt = 0;
    while attempt < COMBINE_RETRIES {
        // println!("Rust: Sending request to server: {}", request_url);
        let response = match client.get(&request_url).send().await {
            Ok(res) => { res },
            Err(e) => {
                eprintln!("Error (NOT saving this as 'Nothing') {e}");
                continue;
            }
        };

        // only count the attempt if it actually successfully communicated with the local:3000 server.
        attempt += 1;

        let status = response.status();
        let response_text = response.text().await.expect("could not get response.text()"); // Get body text
        // println!("Rust: Received status: {}", status);

        if status.is_success() {
            // Try parsing as the success response
            match serde_json::from_str::<CombineResponse>(&response_text) {
                Ok(data) => {
                    return Some(data);
                }
                Err(e) => {
                    eprintln!("Rust: Failed to parse SUCCESS JSON: {e}. JSON TEXT: {response_text}");
                },
            }
        } else {
            // eprintln!("Rust: Request failed: {status}");
        }
    }

    None
}






impl RecipesState {
    pub async fn process_all_to_request_recipes(&mut self, name: &str) {
        let request_stats_arc = Arc::new(Mutex::new(RequestStats {
            to_request: self.to_request_recipes.len(),
            outgoing_requests: 0,
            responded_requests: 0,
            start_time: Instant::now(),
            name: name.to_string()
        }));

        let rs_clone = Arc::clone(&request_stats_arc);
        let interval_task = tokio::spawn(async move {
            let mut interval_timer = tokio::time::interval(COMBINE_INTERVAL_MESSAGE_SECS);
            loop {
                interval_timer.tick().await;
                interval_message(&rs_clone.lock().expect("Interval lock poisoned"));
            }
        });
        
        let to_request_recipes = std::mem::take(&mut self.to_request_recipes);
        let num_to_str_clone_arc = Arc::new(self.num_to_str.clone());

        let mut stream = futures::stream::iter(to_request_recipes)
            .map(|(f, s)| {
                let rs_clone = Arc::clone(&request_stats_arc);
                let num_to_str_clone = Arc::clone(&num_to_str_clone_arc);

                task::spawn(async move {
                    rs_clone.lock().expect("Outgoing lock poisoned").outgoing_requests += 1;
    
                    let first_str = num_to_str_clone[f as usize].clone();
                    let second_str = num_to_str_clone[s as usize].clone();
                    let result_str = combine(&first_str, &second_str).await
                        .map_or_else(|| String::from("Nothing"), |res| res.result);
    
                    (first_str, second_str, result_str)
                })
            })
            // this makes sure that not all tasks are spawned at once, it is limited
            .buffer_unordered(MAX_CONCURRENT_REQUESTS);

        
        let mut str_to_num = self.get_str_to_num_map();

        loop {
        tokio::select! {
            // BRANCH 1: a request to local:3000 finished
            result = stream.next() => {
                if let Some(task_result) = result {
                    match task_result {
                        Ok((first_str, second_str, result_str)) => {
                            self.variables_add_recipe(&first_str, &second_str, &result_str, &mut str_to_num);
                        },
                        Err(join_err) => {
                            eprintln!("Task panicked or was cancelled: {join_err}");
                        },
                    }
                    request_stats_arc.lock().expect("rs lock poisoned").responded_requests += 1;
                
                    self.recipes_updated_total += 1;
                    if let Some(auto_save) = &self.auto_save
                    && self.recipes_updated_total % auto_save.every_changed_recipes + 1 == 0 {
                        self.auto_save();
                    }
                } else {
                    // `futures.next()` returned None, meaning all requests are done.
                    break;
                }
            }

            // BRANCH 2: user pressed ctrl+c
            _ = tokio::signal::ctrl_c() => {
                println!("\n[!] Ctrl+C detected! Canceling remaining requests... (it should hopefully autosave in main now.)");
                return;
            }
        }
        }

        let rs = request_stats_arc.lock().expect("Final lock poisoned");
        interval_message(&rs);
        interval_task.abort();
    }
}





fn interval_message(rs: &RequestStats) {
    println!("{} Requests: {}/{},  Time: {},  Current Outgoing: {},  Rps: {}",
        rs.name,
        rs.responded_requests.to_formatted_string(&num_format::Locale::en).green(),
        (rs.to_request).to_formatted_string(&num_format::Locale::en).green(),

        format!("{:?}", rs.start_time.elapsed()).green(),

        (rs.outgoing_requests - rs.responded_requests).to_string().green(),
        
        format!("{:.3}", rs.responded_requests as f64 / rs.start_time.elapsed().as_secs_f64()).green(),
    );
}