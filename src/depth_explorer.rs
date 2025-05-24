use std::collections::hash_map;
use std::sync::Arc;
use std::time::Instant;
use std::cmp;
use std::fmt::Write as FmtWrite;
use std::fs::{self, File};
use std::io::{self, Write, BufWriter};
use std::path::PathBuf;
use std::cell::RefCell;
use dashmap::DashSet;
use rustc_hash::{FxHashMap, FxHashSet};
use async_recursion::async_recursion;
use rayon::prelude::*;
use tinyvec::ArrayVec;  // can't move to heap
use colored::Colorize;

use crate::{lineage::*, DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS, DEPTH_EXPLORER_MAX_SEED_LENGTH};
use crate::structures::*;
use crate::recipe_requestor::process_all_to_request_recipes;





pub type Seed = ArrayVec<[u32; DEPTH_EXPLORER_MAX_SEED_LENGTH - 1]>;
type EncounteredMap = FxHashMap<Element, Vec<Seed>>;
type ElementBaseCacheMap = Vec<Option<Vec<Element>>>;



thread_local! {
    // Pool of HashSets per thread
    static ALL_RESULTS_POOL: RefCell<Vec<Vec<u32>>> = RefCell::new(Vec::new());
}







#[derive(Default, Clone)]
pub struct DepthExplorerVars {
    pub lineage_elements: Vec<Element>,
    pub stop_after_depth: usize,
    pub exclude_depth1_elements: Vec<Element>,
    pub split_start: usize,
    pub disable_depth_logs: bool,
}



struct DepthExplorerPrivateStructures {
    base_lineage_vec: Vec<u32>,
    depth1: FxHashSet<u32>,
    base_lineage_depth1: FxHashSet<u32>,

    next_seed_set: DashSet<Seed>,
    encountered: Arc<EncounteredMap>,
    
    element_base_cache: Arc<ElementBaseCacheMap>,
    num_to_str_len: Vec<usize>,

    start_time: Instant,
}







#[async_recursion]
pub async fn depth_explorer_split_start(de_vars: &DepthExplorerVars) -> EncounteredMap {
    if de_vars.split_start == 0 { return depth_explorer_start(&de_vars).await; }


    let start_time = Instant::now();

    // calculate depth 1
    let mut initial_split_de_vars = de_vars.clone();
    initial_split_de_vars.stop_after_depth = 1;
    let initial_split_encountered = depth_explorer_start(&mut initial_split_de_vars).await;

    // iterate over every 1-step element
    let total_to_process = initial_split_encountered.len();

    let process_element = async |element: Element, excluded_depth1_elements: Vec<u32>| {
        println!("{} Split Depth Explorer - {}/{}: {} - Time: {}",
            de_vars.split_start.to_string().purple(),
            (excluded_depth1_elements.len() + 1).to_string().purple(),
            total_to_process.to_string().purple(),
            num_to_str_fn(element).purple(),
            format!("{:?}", start_time.elapsed()).yellow(),
        );

        // create new de_vars starting from every 1-step
        let mut new_de_vars = de_vars.clone();
        new_de_vars.lineage_elements.push(element);
        new_de_vars.stop_after_depth -= 1;
        new_de_vars.split_start -= 1;
        new_de_vars.exclude_depth1_elements = excluded_depth1_elements;
        new_de_vars.disable_depth_logs = true;

        let new_encountered = if new_de_vars.split_start == 0
        { depth_explorer_start(&new_de_vars).await }
        else { depth_explorer_split_start(&new_de_vars).await };

        let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
        let element_ic = variables.neal_case_map.read().unwrap()[element as usize];

        let new_extended_encountered = extend_encountered_seeds(new_encountered, element_ic);
        new_extended_encountered
    };

    
    // sequential processing
    let mut collected_encountereds = initial_split_encountered.clone();
    let mut excluded_depth1s = Vec::new();
    for element in initial_split_encountered.into_keys() {
        let element_encountered = process_element(element, excluded_depth1s.clone()).await;
        collected_encountereds = merge_encountered_maps(collected_encountereds, element_encountered);
        excluded_depth1s.push(element);
    }




    if !de_vars.disable_depth_logs {
        println!("finished split depth explorer: {},  Elements: {}",
            format!("{:?}", start_time.elapsed()).red(),
            collected_encountereds.len().to_string().red()
        );
    }
    collected_encountereds
}







fn extend_encountered_seeds(mut encountered: EncounteredMap, add_element: Element) -> EncounteredMap {
    encountered
        .iter_mut()
        .for_each(|(_, original_seeds)| {
            original_seeds
                .iter_mut()
                    .for_each(|original_seed| {
                        let insertion_point = original_seed.binary_search(&add_element).unwrap_or_else(|idx| idx);
                        original_seed.insert(insertion_point, add_element);
                    });
        });
    encountered
}













pub async fn depth_explorer_start(de_vars: &DepthExplorerVars) -> EncounteredMap {
    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
    
    let base_lineage_vec: Vec<Element> = BASE_IDS.chain(de_vars.lineage_elements.iter().copied()).collect();
    let base_lineage_vec_ic: Vec<Element>;
    {
        let neal_case_map = variables.neal_case_map.read().unwrap();
        base_lineage_vec_ic = base_lineage_vec.iter().map(|&x| neal_case_map[x as usize]).collect();
    }
    // --- Private Structures ---
    let mut de_struc = DepthExplorerPrivateStructures {
        base_lineage_depth1: base_lineage_vec.iter().copied().collect(),
        base_lineage_vec: base_lineage_vec_ic,
        depth1: FxHashSet::default(),

        next_seed_set: DashSet::default(),
        encountered: Arc::new(FxHashMap::default()),

        element_base_cache: Arc::new(Vec::new()),
        num_to_str_len: get_num_to_str_len(),

        start_time: Instant::now(),
    };




    // --- Depth 1 ---
    {
        let mut depth1 = Vec::new();
        all_combination_results(&de_struc.base_lineage_vec, &mut depth1, &de_struc, &variables.recipes_ing.read().unwrap(), false);
    
        let empty_seed = Seed::default();
        let empty_encountered_arc = Arc::new(EncounteredMap::default());
        for &x in depth1.iter().filter(|x| !de_vars.exclude_depth1_elements.contains(x)) {
            add_to_local_encountered(x, &empty_seed, Arc::get_mut(&mut de_struc.encountered).unwrap(), &empty_encountered_arc);
        }

        let neal_case_map = variables.neal_case_map.read().unwrap();
        let depth1_ic: Vec<Element> = depth1
            .into_iter()
            .map(|x| neal_case_map[x as usize])
            .filter(|&x| de_struc.num_to_str_len[x as usize] <= 30)
            .collect();

        de_struc.base_lineage_depth1.extend(depth1_ic.iter());

        de_struc.depth1 = depth1_ic.into_iter().filter(|x| !de_vars.exclude_depth1_elements.contains(x)).collect();
        de_struc.next_seed_set = de_struc.depth1.iter().map(|&x| {
            let mut seed = Seed::new();
            seed.push(x);
            seed
        }).collect();
    }
    


    


    
    // --- Main Loop ---
    let mut depth = 1;
    while depth < de_vars.stop_after_depth {

        // --- do all Element - Base combinations and cache them ---
        cache_all_element_base_results(&mut de_struc, depth);


        let past_seed_set = de_struc.next_seed_set;

        let final_depth = depth + 1 == de_vars.stop_after_depth;
        de_struc.next_seed_set = if final_depth { DashSet::default() }
        else { DashSet::with_capacity(past_seed_set.len() * DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS) };


        // --- Main Processing Loop ---
        let new_encountered;
        {
            let neal_case_map = variables.neal_case_map.read().unwrap();
            let recipes_ing = variables.recipes_ing.read().unwrap();

            new_encountered = past_seed_set
                .par_iter()
                .fold(
                    EncounteredMap::default,
                    |mut local_encountered, seed| {
                        proccess_seed_logic(&seed, &de_struc, &mut local_encountered, depth, final_depth, &neal_case_map, &recipes_ing);
                        local_encountered
                    }
                )
                .reduce(
                    EncounteredMap::default,
                    merge_encountered_maps
                );
        }

        // -- Sequential Merge --
        let old_encountered = Arc::try_unwrap(de_struc.encountered).unwrap();
        de_struc.encountered = Arc::new(merge_encountered_maps(old_encountered, new_encountered));
        
        
        if variables.to_request_recipes.is_empty() {
            depth += 1;
            de_struc.next_seed_set.shrink_to_fit();

            if !de_vars.disable_depth_logs {
                println!("Depth {} complete!  Time: {},  Elements: {},  Seeds: {} -> {}",
                    depth.to_string().yellow(),
                    format!("{:?}", de_struc.start_time.elapsed()).yellow(),
                    de_struc.encountered.len().to_string().yellow(),
                    past_seed_set.len().to_string().yellow(),
                    de_struc.next_seed_set.len().to_string().yellow(),
                );
            }
        }
        else {
            println!("Depth {} paused. Requesting {} new recipes...", depth + 1, variables.to_request_recipes.len());
            Arc::get_mut(&mut de_struc.element_base_cache).unwrap().clear();
            de_struc.next_seed_set = past_seed_set;
            process_all_to_request_recipes().await;
            de_struc.num_to_str_len = get_num_to_str_len();
        }
    }

    Arc::try_unwrap(de_struc.encountered).unwrap()
}












fn proccess_seed_logic(
        seed: &Seed,
        de_struc: &DepthExplorerPrivateStructures,
        local_encountered: &mut EncounteredMap,
        depth: usize,
        final_depth: bool,
        neal_case_map: &Vec<u32>,
        recipes_ing: &FxHashMap<(Element, Element), Element>,
    ) {


    // all seed-seed and seed-base combinations
    let mut all_results = ALL_RESULTS_POOL.with(|pool_cell| {
        pool_cell.borrow_mut().pop().unwrap_or_else(|| {
            Vec::new()
        })
   });
   // clear before use
   all_results.clear();

    all_combination_results(seed, &mut all_results, de_struc, recipes_ing, true);

    // for some reason its faster without deduplication...
    // all_results.sort_unstable();
    // all_results.dedup();



    if final_depth {
        for &result in all_results.iter() {
            add_to_local_encountered(result, seed, local_encountered, &de_struc.encountered);
        }
    }


    else {
        let mut count_depth1s: i32 = 0;

        for result in seed.iter() {
            if de_struc.depth1.contains(result) { count_depth1s += 1; }
        }

        // if seed doesn't have too many depth1s already (crazy formula i know)
        if 3*(count_depth1s + 1) - 2*(depth as i32) <= 4 {

            // extend seed with depth1 elements
            for d1 in de_struc.depth1.iter() {
                if !seed.contains(d1) {
                    let mut new_seed = seed.clone();
                    let insertion_point = new_seed.binary_search(d1).unwrap_or_else(|idx| idx);
                    new_seed.insert(insertion_point, *d1);

                    de_struc.next_seed_set.insert(new_seed);
                }
            }
        }


        for &result in all_results.iter() {
            add_to_local_encountered(result, seed, local_encountered, &de_struc.encountered);

            if de_struc.num_to_str_len[result as usize] > 30 { continue; }
        

            // eliminate seeds with too many depth1s
            // this was really difficult to come up with.
            // it definitely does not eliminate all useless seeds, but its the best i can do without storing recipes inside each seed
            // (count_depth1s - (2 * (depth + 1 - count_depth1s)) <= 2)
            // = (3*count_depth1s - 2*depth <= 4)

            // (count_depth1s + if depth1.contains(&result) {1} else {0}) = total depth1s in the full seed.
            let result_ic = neal_case_map[result as usize];

            if 3*(count_depth1s + (if de_struc.depth1.contains(&result_ic) {1} else {0})) - 2*(depth as i32) <= 4
                && !seed.contains(&result_ic) {
                
                let mut new_seed = seed.clone();
                let insertion_point = new_seed.binary_search(&result_ic).unwrap_or_else(|idx| idx);
                new_seed.insert(insertion_point, result_ic);
                
                de_struc.next_seed_set.insert(new_seed);
            }
        }                
    }

    // done with all_results
    ALL_RESULTS_POOL.with(|pool_cell| {
        let mut pool = pool_cell.borrow_mut();
        pool.push(all_results);
   });
}

























fn all_combination_results(
    input_seed: &[Element],
    results_vec: &mut Vec<Element>,
    de_struc: &DepthExplorerPrivateStructures,
    recipes_ing: &FxHashMap<(Element, Element), Element>,
    use_cache: bool
) {
    for (i, &ing1) in input_seed.iter().enumerate() {
        // all seed-seed combinations
        for &ing2 in input_seed.iter().take(i + 1) {
            let comb = sort_recipe_tuple((ing1, ing2));
            if let Some(&result) = recipes_ing.get(&comb) {
                if result != NOTHING_ID && !de_struc.base_lineage_depth1.contains(&result) {
                    results_vec.push(result);
                }
            }
            else { 
                // comb does not exist, add it to the requests
                let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
                variables.to_request_recipes.insert(comb);
            }
        }

        if use_cache {
            let ing1_with_base_cache = de_struc.element_base_cache[ing1 as usize].as_ref().unwrap_or_else(|| panic!("{} not in cache", num_to_str_fn(ing1)));
            results_vec.extend(ing1_with_base_cache);
        }
    }
}









fn cache_all_element_base_results(de_struc: &mut DepthExplorerPrivateStructures, depth: usize) {
    // let start_time = Instant::now();

    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized");
    let neal_case_map = variables.neal_case_map.read().unwrap();


    // resize element_base_cache vec to the max ID
    let mut_element_base_cache = Arc::get_mut(&mut de_struc.element_base_cache).unwrap();
    mut_element_base_cache.resize(neal_case_map.len(), None);


    for (&element, seeds) in de_struc.encountered.iter() {
        if de_struc.num_to_str_len[element as usize] > 30 || seeds.first().unwrap().len() >= depth { continue; }
        let neal_element = neal_case_map[element as usize];

        if mut_element_base_cache[neal_element as usize].is_none() {
            // cache results
            let mut cache_results = Vec::new();

            for &base_element in de_struc.base_lineage_vec.iter() {
                let comb = sort_recipe_tuple((neal_element, base_element));
                if let Some(&result) = recipes_ing.get(&comb) {
                    if result != NOTHING_ID && !de_struc.base_lineage_depth1.contains(&result) && !cache_results.contains(&result) {
                        cache_results.push(result);
                    }
                }
                else { 
                    // comb does not exist, add it to the requests
                    variables.to_request_recipes.insert(comb);
                }
            }

            mut_element_base_cache[neal_element as usize] = Some(cache_results);
        }
    }

    // println!("finished caching element - base: {:?}", start_time.elapsed());
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
                let mut new_vec = Vec::new();
                new_vec.push(seed.clone());
                entry.insert(new_vec);
            }
        }
    }
}




fn merge_encountered_maps(mut main_map: EncounteredMap, mut merge_map: EncounteredMap) -> EncounteredMap {
    if merge_map.len() > main_map.len() {
        (main_map, merge_map) = (merge_map, main_map);
    }

    for (element, local_seeds) in merge_map {
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














pub fn get_encountered_entry(
    element: Element,
    seeds: &[Seed],
    initial_crafted: &FxHashSet<Element>,
    real_recipes_result: &FxHashMap<Element, Vec<(Element, Element, Option<Element>)>>
) -> String {

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
                        .find(|&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1)) {
                        
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
            if !changes { panic!("could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, debug_element_vec(&to_craft)); }

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







pub fn generate_lineages_file(de_vars: &DepthExplorerVars, encountered: EncounteredMap) -> io::Result<()> {
    // required for this function:
    let start_time = Instant::now();

    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let neal_case_map = variables.neal_case_map.read().unwrap();
    let mut real_recipes_result: FxHashMap<Element, Vec<(Element, Element, Option<Element>)>> =FxHashMap::with_capacity_and_hasher(
        neal_case_map.len(), Default::default()
    );

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


    let base_lineage_vec: Vec<u32> = BASE_IDS.chain(de_vars.lineage_elements.iter().copied()).collect();
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
    let keyed_entries: Vec<(usize, u32, String)> = encountered
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
    writeln!(writer, "{}  // {}\n\n\n", "TODO...", base_lineage_vec.len() - 4)?;

    for (i, bucket) in final_buckets.iter().enumerate() {
        writeln!(writer, "{} Steps - {} Elements", i + 1, bucket.len())?;
    }
    writeln!(writer, "Total Elements: {}\n\n\n\n", encountered.len())?;



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