use std::collections::hash_map;
use std::time::Instant;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, Write, BufWriter};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use arc_swap::ArcSwap;
use rayon::prelude::*;
use dashmap::{DashMap, DashSet};
use tinyvec::TinyVec;   // can move to heap
use tinyvec::ArrayVec;  // can't move to heap
use colored::Colorize;
use std::cell::RefCell;

use crate::{main, GLOBAL_OPTIONS};
use crate::lineage::*;
use crate::structures::*;
use crate::recipe_requestor::process_all_to_request_recipes;





const DEPTH_GROW_FACTOR: usize = 7;
pub type Seed = ArrayVec<[u32; GLOBAL_OPTIONS.depth_explorer_max_seed_length - 1]>;



thread_local! {
    // Pool of HashSets per thread
    static ALL_RESULTS_POOL: RefCell<Vec<FxHashSet<u32>>> = RefCell::new(Vec::new());
}











#[derive(Default)]
pub struct DepthExplorerVars<'a> {
    pub input_text_lineage: &'a str,
    pub stop_after_depth: usize,
    pub encountered: FxHashMap<u32, TinyVec<[Seed; 5]>>
}

type EncounteredMap = FxHashMap<Element, TinyVec<[Seed; 5]>>;


struct DepthExplorerPrivateStructures {
    base_lineage_vec: Vec<u32>,
    depth1: FxHashSet<u32>,
    base_lineage_depth1: FxHashSet<u32>,

    seed_sets: Vec<DashSet<Seed>>,
    main_encountered: ArcSwap<EncounteredMap>,
    
    element_combined_with_base: DashMap<u32, Vec<u32>>,
    num_to_str_len: Vec<usize>,

    start_time: Instant,
    processed_seed_count: AtomicUsize,
}







pub async fn depth_explorer_start(de_vars: &mut DepthExplorerVars<'_>) {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let base_elements = variables.base_elements;
    

    let base_lineage_vec: Vec<u32> = base_elements.iter().chain(&string_lineage_results(de_vars.input_text_lineage)).copied().collect();
    // --- Private Structures ---
    let mut de_struc = DepthExplorerPrivateStructures {
        base_lineage_depth1: base_lineage_vec.iter().copied().collect(),
        base_lineage_vec,
        depth1: FxHashSet::default(),

        seed_sets: vec![DashSet::default()],
        main_encountered: ArcSwap::from(Arc::new(FxHashMap::with_capacity_and_hasher(GLOBAL_OPTIONS.depth_explorer_final_elements_guess, Default::default()))),

        element_combined_with_base: DashMap::with_capacity(GLOBAL_OPTIONS.depth_explorer_final_elements_guess / GLOBAL_OPTIONS.depth_explorer_depth_grow_factor_guess),
        num_to_str_len: variables.num_to_str.read().unwrap()
                .iter()
                .map(|x| x.len())
                .collect(),

        start_time: Instant::now(),
        processed_seed_count: AtomicUsize::new(0),
    };




    // --- Depth 1 ---
    let mut depth1 = FxHashSet::default();
    all_combination_results(&de_struc.base_lineage_vec, &mut depth1, &de_struc, false);
    let mut depth1_map = EncounteredMap::default();
    let empty_arc = Arc::new(EncounteredMap::default());
    for &x in depth1.iter() {
        add_to_local_encountered(x, &Seed::new(), &mut depth1_map, &empty_arc);
    }
    de_struc.main_encountered.store(Arc::new(depth1_map));
    de_struc.base_lineage_depth1.extend(depth1.iter());
    de_struc.depth1 = depth1;



    // add the first seed
    de_struc.seed_sets[0].insert(Seed::new());


    


    
    // --- Main Loop ---
    let mut depth = 0;
    while depth < de_vars.stop_after_depth {
        let final_depth = depth + 1 == de_vars.stop_after_depth;

        println!("now processing depth {}", depth + 1);

        if !final_depth && de_struc.seed_sets.get(depth + 1).is_none() {
            de_struc.seed_sets.push(DashSet::with_capacity(de_struc.seed_sets[depth].len() * DEPTH_GROW_FACTOR));
        }

        let seeds = de_struc.seed_sets
            .get(depth)
            .expect("depth larger than seed_set");



        // --- Parallel Processing with Fold/Reduce ---
        let depth_results_map = seeds
            .into_par_iter()
            .fold(
                // Initial value factory: Each thread gets an empty local map
                || EncounteredMap::default(),
                // Fold operation: Process seed, check shared map, update local map
                |mut local_encountered_map, seed| { // seed is owned
                    // Pass the Arc'd shared map to the processing logic
                    proccess_seed_logic(
                        seed,
                        depth,
                        &de_struc,
                        final_depth,
                        &mut local_encountered_map,     // Pass local write map
                    );
                    local_encountered_map // Return the updated local map
                }
            )
            // Reduce operation: Combine local maps using the merge helper
            .reduce(
                || EncounteredMap::default(), // Identity for merging
                merge_local_encountered_maps // Use the merge helper
            );
        // --- End Fold/Reduce ---

        // --- Merge and Update ---
        // 1. Load the *current* Arc pointer
        let current_main_arc = de_struc.main_encountered.load_full();
        // 2. Clone the data *from the current Arc*
        let current_main_map_data = (*current_main_arc).clone();
        // 3. Merge the depth results into the *cloned* data
        let new_main_map_data = merge_local_encountered_maps(current_main_map_data, depth_results_map);
        // 4. Create a *new Arc* pointing to the merged data
        let new_main_arc = Arc::new(new_main_map_data);
        // 5. Atomically store the new Arc into the ArcSwap
        de_struc.main_encountered.store(new_main_arc);
        

        if variables.to_request_recipes.is_empty() {
            println!("\nDepth {} complete!\n - Time: {:?}\n - Elements: {}\n - Seeds: {} -> {}\n",
                depth + 1,
                de_struc.start_time.elapsed(),
                de_struc.main_encountered.load().len(),
                de_struc.seed_sets[depth].len(),
                if let Some(x) = de_struc.seed_sets.get(depth + 1) { x.len() } else { 0 },
            );

            // clear past seed set
            de_struc.seed_sets[depth].clear();
            // shrink upcoming one to fit
            if !final_depth { de_struc.seed_sets[depth + 1].shrink_to_fit(); }
            depth += 1;
        }
        else {
            println!("Depth {} paused. Requesting {} new recipes...", depth + 1, variables.to_request_recipes.len());
            process_all_to_request_recipes().await;
            de_struc.element_combined_with_base.clear();
        }
        de_struc.processed_seed_count.store(0, Ordering::Relaxed);
    }

    let final_arc = de_struc.main_encountered.load_full();
    de_vars.encountered = Arc::try_unwrap(final_arc)
        .unwrap_or_else(|arc| (*arc).clone()); // Clone if other Arcs somehow still exist
}












fn proccess_seed_logic(
        seed: dashmap::setref::multiple::RefMulti<'_, Seed>,
        depth: usize, de_struc: &DepthExplorerPrivateStructures,
        final_depth: bool,
        local_encountered_map: &mut EncounteredMap
    ) {


    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let neal_case_map = variables.neal_case_map.read().expect("neal_case_map read lock");
    let shared_read_encountered_map = de_struc.main_encountered.load();


    // all seed-seed and seed-base combinations
    let mut all_results = ALL_RESULTS_POOL.with(|pool_cell| {
        pool_cell.borrow_mut().pop().unwrap_or_else(|| {
            FxHashSet::with_capacity_and_hasher(25, Default::default())
        })
   });
   // clear before use
   all_results.clear();

    all_combination_results(&seed, &mut all_results, de_struc, !final_depth);



    if final_depth {
        for &result in all_results.iter() {
            add_to_local_encountered(result, &seed, local_encountered_map, &shared_read_encountered_map);
        }
    }


    else {
        let mut count_depth1s: i32 = 0;

        for result in seed.iter() {
            if de_struc.depth1.contains(result) { count_depth1s += 1; }
        }

        // if seed doesn't have too many depth1s already (crazy formula i know)
        if 3*(count_depth1s + 1) - 2*(depth as i32) <= 4 {
            // extend seeds with depth1 elements
            let next_seed_set = &de_struc.seed_sets[depth + 1]; // Get DashSet reference

            for d1 in de_struc.depth1.iter() {
                if !seed.contains(d1) {
                    let mut new_seed = seed.clone();
                    let insertion_point = new_seed.binary_search(d1).unwrap_or_else(|idx| idx);
                    new_seed.insert(insertion_point, *d1);

                    next_seed_set.insert(new_seed);
                }
            }
        }


        for &result in all_results.iter() {
            add_to_local_encountered(result, &seed, local_encountered_map, &shared_read_encountered_map);

            if de_struc.num_to_str_len[result as usize] > 30 { continue; }
        

            // eliminate seeds with too many depth1s
            // this was really difficult to come up with, but it does not eliminate all useless seeds, there is definitely improvement here, by checking unused recipes
            // (count_depth1s - (2 * (depth + 1 - count_depth1s)) <= 2)
            // = (3*count_depth1s - 2*depth <= 4)

            // count_depth1s + if depth1.contains(&result) {1} else {0}) = total depth1s in the full seed.
            let new_result = *neal_case_map.get(result as usize).expect("result not in neal_case_map");

            if 3*(count_depth1s + (if de_struc.depth1.contains(&new_result) {1} else {0})) - 2*(depth as i32) <= 4
                && !seed.contains(&new_result) {
                
                let mut new_seed = seed.clone();
                let insertion_point = new_seed.binary_search(&new_result).unwrap_or_else(|idx| idx);
                new_seed.insert(insertion_point, new_result);
                
                de_struc.seed_sets[depth + 1].insert(new_seed);
            }
        }                
    }

    // done with all_results
    ALL_RESULTS_POOL.with(|pool_cell| {
        let mut pool = pool_cell.borrow_mut();
        pool.push(all_results);
   });

   de_struc.processed_seed_count.fetch_add(1, Ordering::Relaxed);
}






















fn all_combination_results(input_seed: &[u32], existing_set: &mut FxHashSet<u32>, de_struc: &DepthExplorerPrivateStructures, cache_element_base: bool) {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized");

    let base_lineage_depth1 = &de_struc.base_lineage_depth1;
    let element_combined_with_base = &de_struc.element_combined_with_base;



    for (i, &ing1) in input_seed.iter().enumerate() {
        // all seed-seed combinations
        for &ing2 in input_seed.iter().take(i + 1) {
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

                    for &base_element in de_struc.base_lineage_vec.iter() {
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
                    for &base_element in de_struc.base_lineage_vec.iter() {
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










fn add_to_local_encountered(element: Element, seed: &Seed, local_map: &mut EncounteredMap, shared_read_encountered_map: &Arc<EncounteredMap>) {
    let mut update = false;

    if let Some(existing_seeds) = shared_read_encountered_map.get(&element) {
        match seed.len().cmp(&existing_seeds.first().unwrap().len()) {
            cmp::Ordering::Less => {
                // new seed is shorter, replace the list
                update = true;
            },
            cmp::Ordering::Equal => {
                // new seed is same length, add if not already present (linear scan ok for few ties)
                if !existing_seeds.contains(seed) {
                    update = true;
                }
            },
            cmp::Ordering::Greater => {
                // new seed is longer, do nothing
            },
        }
    }
    else { update = true; }




    if update {
        match local_map.entry(element) {
            hash_map::Entry::Occupied(mut entry) => {
                let existing_seeds = entry.get_mut();

                match seed.len().cmp(&existing_seeds.first().unwrap().len()) {
                    cmp::Ordering::Less => {
                        // new seed is shorter, replace the list
                        existing_seeds.clear();
                        existing_seeds.push(seed.clone());
                    },
                    cmp::Ordering::Equal => {
                        // new seed is same length, add if not already present (linear scan ok for few ties)
                        if !existing_seeds.contains(seed) {
                            existing_seeds.push(seed.clone());
                        }
                    },
                    cmp::Ordering::Greater => {
                        // new seed is longer, do nothing
                    },
                }
            }
            hash_map::Entry::Vacant(entry) => {
                // element is new for this thread
                let mut new_vec = TinyVec::new();
                new_vec.push(seed.clone());
                entry.insert(new_vec);
            }
        }
    }
}




fn merge_local_encountered_maps(mut main_map: EncounteredMap, local_map: EncounteredMap) -> EncounteredMap {
    for (element, local_seeds) in local_map {
        match main_map.entry(element) {
            hash_map::Entry::Occupied(mut entry) => {
                let main_seeds = entry.get_mut();
                let main_len = main_seeds.first().unwrap().len(); // Assumes non-empty
                let local_len = local_seeds.first().unwrap().len();

                match local_len.cmp(&main_len) {
                    cmp::Ordering::Less => {
                        // Local map's seeds are shorter, replace main's
                        *main_seeds = local_seeds;
                    }
                    cmp::Ordering::Equal => {
                        // Same length, add seeds from local map if not already present
                        for seed in local_seeds.into_iter() {
                            if !main_seeds.contains(&seed) {
                                main_seeds.push(seed);
                            }
                        }
                    }
                    cmp::Ordering::Greater => {
                        // Main map's seeds are shorter, do nothing with this element
                    }
                }
            }
            hash_map::Entry::Vacant(entry) => {
                entry.insert(local_seeds);
            }
        }
    }
    main_map
}














pub fn get_encountered_entry(element: Element, seeds: &[Seed], initial_crafted: &FxHashSet<Element>, real_recipes_result: &FxHashMap<Element, Vec<(Element, Element, Option<Element>)>>) -> String {
    let mut message = String::with_capacity(seeds.len() * seeds[0].len() * 7);    
    write!(message, "{} - {}:", seeds[0].len() + 1, num_to_str_fn(element)).unwrap();

    for (i, seed) in seeds.iter().enumerate() {        

        let mut lineage: Vec<[Element; 3]> = Vec::with_capacity(seed.len() + 1);
        let mut to_craft: Vec<Element> = seed.iter().copied().collect();
        let mut crafted: FxHashSet<Element> = initial_crafted.clone();
        let mut caps_map: FxHashMap<Element, Element> = FxHashMap::default();

        while !to_craft.is_empty() {
            let mut changes = false;

            to_craft = to_craft
                .iter()
                .filter(|&to_craft_element| {
                    if let Some(recipe) = real_recipes_result
                        .get(to_craft_element)
                        .expect("to_craft_element not in real_recipes_result")
                        .iter()
                        .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1)) {
                        
                        crafted.insert(*to_craft_element);

                        if let Some(actual_caps_result) = recipe.2 {
                            caps_map.insert(*to_craft_element, actual_caps_result);
                        }
                        
                        lineage.push([
                            *caps_map.get(&recipe.0).unwrap_or_else(|| &recipe.0),
                            *caps_map.get(&recipe.1).unwrap_or_else(|| &recipe.1),
                            *caps_map.get(to_craft_element).unwrap_or_else(|| to_craft_element),
                        ]);
                        changes = true;
                        false  // filter out
                    }
                    else { true }  // keep
                })
                .copied()
                .collect();

            if !changes { panic!("could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, to_craft); }
        };

        let final_recipe = real_recipes_result
            .get(&element)
            .expect("element not in real_recipes_result")
            .iter()
                // find with correct caps
            .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1) && rec.2.unwrap_or_else(|| element) == element)
            .expect("could not find a final_recipe...");

        lineage.push([
            *caps_map.get(&final_recipe.0).unwrap_or_else(|| &final_recipe.0),
            *caps_map.get(&final_recipe.1).unwrap_or_else(|| &final_recipe.1),
            element
        ]);


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







pub fn generate_lineages_file(de_vars: &DepthExplorerVars) -> io::Result<()> {
    // required for this function:
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let neal_case_map = variables.neal_case_map.read().unwrap();
    let mut real_recipes_result: FxHashMap<Element, Vec<(Element, Element, Option<Element>)>> = FxHashMap::with_capacity_and_hasher(neal_case_map.len(), Default::default());

    for (&(first, second), &result) in recipes_ing.iter() {
        let f = neal_case_map[first as usize];
        let s = neal_case_map[second as usize];
        let r = neal_case_map[result as usize];
        if result == r {
            // if result is neal case
            real_recipes_result.entry(r).or_default().push((f, s, None));
        }
        else {
            // if result is not neal case
            real_recipes_result.entry(r).or_default().push((f, s, Some(result)));
            real_recipes_result.entry(result).or_default().push((f, s, None));
        };
    };
    println!("made recipes_result in {:?}", start_time.elapsed());


    let base_elements = variables.base_elements;
    let base_lineage_vec: Vec<u32> = base_elements.iter().chain(&string_lineage_results(de_vars.input_text_lineage)).copied().collect();
    let initial_crafted: FxHashSet<Element> = base_lineage_vec.iter().copied().collect();


    let start_time = Instant::now();

    let folder_name = "Lineages Files";
    let file_name = format!("{} Seed - {} Steps.txt", &num_to_str_fn(*base_lineage_vec.last().unwrap()), de_vars.stop_after_depth);

    let folder_path = PathBuf::from(folder_name);
    fs::create_dir_all(&folder_path)?;

    let full_path = folder_path.join(file_name);

    let file = File::create(full_path)?;
    let mut writer = BufWriter::new(file);

    // --- Parallel Processing ---
    let keyed_entries: Vec<(usize, u32, String)> = de_vars.encountered
        .par_iter()
        .map(|(&element, seeds)| {

            let bucket_key = seeds.first().map_or(0, |first_seed| first_seed.len() + 1);
            let formatted_string = get_encountered_entry(element, seeds, &initial_crafted, &real_recipes_result);
            (bucket_key, element, formatted_string)
        })
        .collect();



    // Iterate sequentially through the collected pairs and push into buckets.
    let mut final_buckets: Vec<Vec<(u32, String)>> = vec![Vec::new(); de_vars.stop_after_depth];
    for (key, element, formatted_string) in keyed_entries {
        final_buckets[key - 1].push((element, formatted_string));
    }


    // --- Writing ---
    writeln!(writer, "{}  // {}\n\n\n", de_vars.input_text_lineage.trim(), base_lineage_vec.len() - 4)?;

    for (i, bucket) in final_buckets.iter().enumerate() {
        writeln!(writer, "{} Steps - {} Elements", i + 1, bucket.len())?;
    }
    writeln!(writer, "Total Elements: {}\n\n\n\n", de_vars.encountered.len())?;



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








fn print_interval_message(depth: usize, de_struc: &DepthExplorerPrivateStructures) {
    println!("Depth: {},  Time: {},  Elements: {},  Seeds: {}/{}",
        (depth + 1).to_string().yellow(),
        format!("{:?}", de_struc.start_time.elapsed()).yellow(),
        0, // de_struc.main_encountered.read().unwrap().len().to_string().yellow(),
        de_struc.processed_seed_count.load(Ordering::Relaxed).to_string().yellow(),
        de_struc.seed_sets[depth].len().to_string().yellow(),
    );
}