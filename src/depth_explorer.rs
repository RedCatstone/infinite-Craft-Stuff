use std::sync::OnceLock;
use std::time::Instant;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Ordering;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, Write, BufWriter};
use std::path::PathBuf;
use rayon::prelude::*;
use dashmap::{DashMap, DashSet};
use smallvec::SmallVec;
use tokio::{task::JoinHandle, time::{self, Duration}};
use colored::Colorize;

use crate::{lineage::*, DEPTH_EXPLORER_OPTIONS};
use crate::structures::*;
use crate::recipe_requestor::{process_all_to_request_recipes, REQUEST_STATS};


pub static DEPTH_EXPLORER_VARS: OnceLock<DepthExplorerVars> = OnceLock::new();
pub type Seed = SmallVec<[u32; DEPTH_EXPLORER_OPTIONS.stop_after_depth - 1]>;







#[derive(Debug)]
pub struct DepthExplorerVars {
    pub base_lineage_vec: Vec<u32>,

    pub start_time: Instant,
    pub depth1: FxHashSet<u32>,
    pub base_lineage_depth1: FxHashSet<u32>,

    pub seed_sets: Vec<DashSet<Seed>>,
    pub encountered: DashMap<u32, SmallVec<[Seed; 5]>>,

    pub element_combined_with_base: DashMap<u32, Vec<u32>>,
}


pub struct DepthExplorerOptions {
    pub input_text_lineage: &'static str,
    pub stop_after_depth: usize,
    pub final_elements_guess: usize,
    pub final_seeds_guess: usize,
}



pub async fn depth_explorer_start() {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let base_elements = variables.base_elements;
    

    let base_lineage_vec: Vec<u32> = base_elements.iter().chain(string_lineage_results(DEPTH_EXPLORER_OPTIONS.input_text_lineage).iter()).copied().collect();

    let mut de_vars = DepthExplorerVars {
        base_lineage_depth1: base_lineage_vec.iter().copied().collect(),
        base_lineage_vec,

        start_time: Instant::now(),
        depth1: FxHashSet::default(),
    
        // Initialize DashSets
        seed_sets: (0..DEPTH_EXPLORER_OPTIONS.stop_after_depth)
            .map(|i| DashSet::with_capacity(DEPTH_EXPLORER_OPTIONS.final_seeds_guess / (7 as usize).pow(i as u32)))
            .collect(),
    
        // Initialize DashMaps
        encountered: DashMap::with_capacity(DEPTH_EXPLORER_OPTIONS.final_elements_guess),
        element_combined_with_base: DashMap::with_capacity(DEPTH_EXPLORER_OPTIONS.final_elements_guess / 7),
    };

    let interval_message = interval_message();




    // --- Depth 1 ---
    let mut depth1 = FxHashSet::default();
    all_combination_results(&de_vars.base_lineage_vec, &mut depth1, &de_vars, true);
    de_vars.depth1 = depth1;
    for &x in de_vars.depth1.iter() {
        add_to_encountered(x, &Seed::new(), &de_vars);
    }

    de_vars.base_lineage_depth1 = de_vars.depth1.iter().chain(&de_vars.base_lineage_vec).copied().collect();
    

    DEPTH_EXPLORER_VARS.set(de_vars).unwrap();
    let de_vars = DEPTH_EXPLORER_VARS.get().unwrap();





    // add the first seed
    de_vars.seed_sets[0].insert(Seed::new());


    


    
    // --- Main Loop ---
    let mut depth = 0;
    while depth < DEPTH_EXPLORER_OPTIONS.stop_after_depth {

        println!("now processing depth {}", depth + 1);

        let seeds = de_vars.seed_sets
            .get(depth)
            .expect("depth larger than seed_set");


        seeds.into_par_iter().for_each(|seed| {
            let variables = VARIABLES.get().expect("VARIABLES not initialized");
            let de_vars = DEPTH_EXPLORER_VARS.get().unwrap();
            let neal_case_map = variables.neal_case_map.read().expect("neal_case_map read lock");

            let not_final_depth: bool = depth + 1 < DEPTH_EXPLORER_OPTIONS.stop_after_depth;


            // all seed-seed and seed-base combinations
            let mut all_results = FxHashSet::default();
            all_combination_results(&seed, &mut all_results, de_vars, not_final_depth);



            let mut count_depth1s: i32 = 0;
            if depth + 1 < DEPTH_EXPLORER_OPTIONS.stop_after_depth {
                for result in seed.iter() {
                    if de_vars.depth1.contains(result) { count_depth1s += 1; }
                }

                // if seed doesn't have too many depth1s already (crazy formula i know)
                if 3*(count_depth1s + 1) - 2*(depth as i32) <= 4 {
                    // extend seeds with depth1 elements
                    let next_seed_set = &de_vars.seed_sets[depth + 1]; // Get DashSet reference

                    for d1 in de_vars.depth1.iter() {
                        if !seed.contains(d1) {
                            let mut new_seed = seed.clone();
                            let insertion_point = new_seed.binary_search(d1).unwrap_or_else(|idx| idx);
                            new_seed.insert(insertion_point, *d1);

                            next_seed_set.insert(new_seed);
                        }
                    }
                }
            }


            for result in all_results.into_iter() {
                add_to_encountered(result, &seed, de_vars);
                
                if seed.contains(&result) { continue; }

                

                if depth + 1 < DEPTH_EXPLORER_OPTIONS.stop_after_depth {
                    // extend seeds with new elements
                    let next_seed_set = &de_vars.seed_sets[depth + 1]; // Get DashSet reference

                    // eliminate seeds with too many depth1s
                    // this was really difficult to come up with, but it does not eliminate all useless seeds, there is definitely improvement here, by checking unused recipes
                    // (count_depth1s - (2 * (depth + 1 - count_depth1s)) <= 2)
                    // = (3*count_depth1s - 2*depth <= 4)

                    // count_depth1s + if depth1.contains(&result) {1} else {0}) = total depth1s in the full seed.
                    let new_result = *neal_case_map.get(result as usize).expect("result not in neal_case_map");

                    if 3*(count_depth1s + (if de_vars.depth1.contains(&new_result) {1} else {0})) - 2*(depth as i32) <= 4
                        && !seed.contains(&new_result)
                        && num_to_str_fn(new_result).len() <= 30 {
                        
                        let mut new_seed = seed.clone();
                        let insertion_point = new_seed.binary_search(&new_result).unwrap_or_else(|idx| idx);
                        new_seed.insert(insertion_point, new_result);

                        next_seed_set.insert(new_seed);
                    }
                }                
            }
        });
        

        if variables.to_request_recipes.is_empty() {
            println!("\nDepth {} complete!\n - Time: {:?}\n - Elements: {}\n - Seeds: {} -> {}\n",
                depth + 1,
                de_vars.start_time.elapsed(),
                de_vars.encountered.len(),
                de_vars.seed_sets[depth].len(),
                if let Some(x) = de_vars.seed_sets.get(depth + 1) { x.len() } else { 0 },
            );
            depth += 1;
        }
        else {
            process_all_to_request_recipes().await;
            de_vars.element_combined_with_base.clear();
        }
    }

    // done
    interval_message.abort();
}










fn all_combination_results(input: &[u32], existing_set: &mut FxHashSet<u32>, de_vars: &DepthExplorerVars, cache_element_base: bool) {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized");

    let base_lineage_depth1 = &de_vars.base_lineage_depth1;
    let element_combined_with_base = &de_vars.element_combined_with_base;



    for (i, &ing1) in input.iter().enumerate() {
        // all seed-seed combinations
        for &ing2 in input.iter().take(i + 1) {
            let comb = sort_recipe_tuple((ing1, ing2));
            if let Some(&result) = recipes_ing.get(&comb) {
                if result != 0 && !base_lineage_depth1.contains(&result) {
                    existing_set.insert(result);
                }
            }
            else { 
                // comb does not exist, add it to the requests
                variables.to_request_recipes.insert(comb);
            }
        }

        match element_combined_with_base.entry(ing1) {
            dashmap::Entry::Occupied(occ_entry) => {
                // ing1 combined with base is already cached, so use that
                existing_set.extend(
                    occ_entry.get()
                );
            },
            dashmap::Entry::Vacant(vacant_entry) => {
                // cache ing1 combined with base

                // cache element-base results
                if cache_element_base {
                    let mut cached_seed_base_results: Vec<u32> = Vec::new();

                    for &base_element in de_vars.base_lineage_vec.iter() {
                        let comb = sort_recipe_tuple((ing1, base_element));
                        if let Some(&result) = recipes_ing.get(&comb) {
                            if result != 0 && !base_lineage_depth1.contains(&result) && !cached_seed_base_results.contains(&result) {
                                cached_seed_base_results.push(result);
                                existing_set.insert(result);
                            }
                        }
                        else { 
                            // comb does not exist, add it to the requests
                            variables.to_request_recipes.insert(comb);
                        }
                    }

                    vacant_entry.insert(cached_seed_base_results);
                }
                else {
                    for &base_element in de_vars.base_lineage_vec.iter() {
                        let comb = sort_recipe_tuple((ing1, base_element));
                        if let Some(&result) = recipes_ing.get(&comb) {
                            if result != 0 && !base_lineage_depth1.contains(&result) {
                                existing_set.insert(result);
                            }
                        }
                        else { 
                            // comb does not exist, add it to the requests
                            variables.to_request_recipes.insert(comb);
                        }
                    }
                }
            },

        }
    }
}






fn add_to_encountered(element: u32, seed: &Seed, de_vars: &DepthExplorerVars) {

    let encountered_map = &de_vars.encountered;
    let mut entry = encountered_map.entry(element).or_default();
    let existing_seeds = entry.value_mut();

    if existing_seeds.is_empty() {
        // first time seeing this element
        existing_seeds.push(seed.clone());
    }
    else {
        // already exists, compare lengths
        match seed.len().cmp(&existing_seeds.first().unwrap().len()) {
            Ordering::Less => {
                // new seed is shorter, replace the list
                existing_seeds.clear();
                existing_seeds.push(seed.clone());
            },
            Ordering::Equal => {
                // new seed is same length, add if not already present (linear scan ok for few ties)
                if !existing_seeds.contains(seed) {
                    existing_seeds.push(seed.clone());
                }
            },
            Ordering::Greater => {
                // new seed is longer, do nothing
            },
        }
    }
    // Lock for this entry is released when `entry` goes out of scope
}







pub fn get_encountered_entry(element: u32, seeds: &[Seed]) -> String {
    let mut message = String::with_capacity(seeds.len() * seeds[0].len() * 7);    
    write!(message, "{} - {}:", seeds[0].len() + 1, num_to_str_fn(element)).unwrap();

    let de_vars = DEPTH_EXPLORER_VARS.get().unwrap();
    let base_lineage_vec = &de_vars.base_lineage_vec;
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let neal_case_map = variables.neal_case_map.read().expect("neal_case_map not initialized");

    for (i, seed) in seeds.iter().enumerate() {
        let mut actual_seed = seed.clone();
        actual_seed.push(*neal_case_map.get(element as usize).unwrap());

        let lineage = generate_lineage_from_results(&actual_seed, base_lineage_vec, variables);
        let lineage_string = format_lineage_no_goals(lineage);

        if i != 0 {
            write!(message, " ...").unwrap();
        }
        writeln!(message, "{}", lineage_string).unwrap();
    };
    write!(message, "\n\n").unwrap();

    message.shrink_to_fit();
    message
}







pub fn generate_lineages_file() -> io::Result<()> {
    // required for this function:
    update_recipes_result();

    let de_vars = DEPTH_EXPLORER_VARS.get().unwrap_or_else(|| panic!("Depth Explorer was not started before generating lineages file."));
    let encountered_map = &de_vars.encountered;


    let start_time = Instant::now();

    let folder_name = "Lineages Files";
    let file_name = format!("{} Seed - {}.txt", &num_to_str_fn(*de_vars.base_lineage_vec.last().unwrap()), DEPTH_EXPLORER_OPTIONS.stop_after_depth);

    let folder_path = PathBuf::from(folder_name);
    fs::create_dir_all(&folder_path)?;

    let full_path = folder_path.join(file_name);

    let file = File::create(full_path)?;
    let mut writer = BufWriter::new(file);

    // --- Parallel Processing ---
    let keyed_entries: Vec<(usize, u32, String)> = encountered_map
        .par_iter()
        .map(|entry| {
            let element = *entry.key();
            let lineages = entry.value();

            let bucket_key = lineages.first().map_or(0, |first_seed| first_seed.len() + 1);
            let formatted_string = get_encountered_entry(element, lineages);
            (bucket_key, element, formatted_string)
        })
        .collect();



    // Iterate sequentially through the collected pairs and push into buckets.
    let mut final_buckets: Vec<Vec<(u32, String)>> = vec![Vec::new(); DEPTH_EXPLORER_OPTIONS.stop_after_depth];
    for (key, element, formatted_string) in keyed_entries {
        final_buckets[key - 1].push((element, formatted_string));
    }


    // --- Writing ---
    writeln!(writer, "{}  // {}\n\n\n", DEPTH_EXPLORER_OPTIONS.input_text_lineage.trim(), de_vars.base_lineage_vec.len() - 4)?;

    for (i, bucket) in final_buckets.iter().enumerate() {
        writeln!(writer, "{} Steps - {} Elements", i + 1, bucket.len())?;
    }
    writeln!(writer, "Total Elements: {}\n\n\n\n", encountered_map.len())?;



    for bucket in final_buckets.iter() {
        for (_element, entry_string) in bucket.iter() {
            writer.write_all(entry_string.as_bytes())?;
        }
    }



    // --- JSON Generation ---
    let json_map: FxHashMap<String, usize> = final_buckets
        .iter()
        .enumerate()
        .flat_map(|(depth, bucket)| {
             bucket
                .iter()
                .map(move |(element, _entry_string)| { (num_to_str_fn(*element), depth) })
        })
        .collect();

    let json_string = serde_json::to_string_pretty(&json_map).expect("JSON seriliaziation failed...");

    writer.write_all(b"\n")?;
    writer.write_all(json_string.as_bytes())?;
    writer.write_all(b"\n")?; 




    println!("Generated Lineages File: {:?}", start_time.elapsed());

    Ok(())
}







fn interval_message() -> JoinHandle<()> {
    tokio::spawn(async {
        let mut interval = time::interval(Duration::from_secs(10));
        loop {
            interval.tick().await;

            let de_vars = DEPTH_EXPLORER_VARS.get().expect("DEPTH_EXPLORER_VARS not initizalized");

            if let Some(rs_mutex) = REQUEST_STATS.get() {
                let rs = rs_mutex.lock().expect("lock poisoned");
                println!("Elements: {}, Time: {}\n  Requests: {}/{}, Current Outgoing Requests: {}, Requests/s: (Total: {}, Last 60s: {})",
                    de_vars.encountered.len().to_string().yellow(),
                    format!("{:?}", de_vars.start_time.elapsed()).yellow(),

                    rs.responded_requests.to_string().yellow(),
                    (rs.to_request).to_string().yellow(),
                    (rs.outgoing_requests - rs.responded_requests).to_string().yellow(),
                    format!("{:.3}", rs.responded_requests as f64 / rs.start_time.elapsed().as_secs_f64()).yellow(),
                    format!("{:.3}", rs.rps_tracker.get_rps()).yellow(),
                );
            }
        }
    })
}