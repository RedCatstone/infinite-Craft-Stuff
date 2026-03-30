use std::{cmp, collections::hash_map, fmt::Write as FmtWrite, hash::Hash, sync::Arc, time::Instant, cell::RefCell, fs::{self, File}, io::{self, BufWriter, Write}, path::PathBuf};
use dashmap::DashSet;
use rustc_hash::{FxBuildHasher, FxHashMap, FxHashSet};
use async_recursion::async_recursion;
use rayon::prelude::*;
use tinyvec::ArrayVec;  // can't move to heap
use colored::Colorize;

use crate::{DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS, DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED, DEPTH_EXPLORER_MAX_STEPS, LINEAGES_FILE_COOL_JSON_MODE};
use crate::structures::{Element, RecipesState, BASE_IDS, sort_recipe_tuple, NOTHING_ID};



pub trait IsSeed: Send + Sync {
    fn as_slice(&self) -> &[Element];
    fn len(&self) -> usize {
        self.as_slice().len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Seed {
    pub elems: ArrayVec<[Element; DEPTH_EXPLORER_MAX_STEPS - 1]>
}
impl Seed {
    pub fn add_element(&mut self, element: Element) {
        let insertion_point = self.elems.binary_search(&element).unwrap_or_else(|i| i);
        self.elems.insert(insertion_point, element);
    }
    pub fn len(&self) -> usize { self.elems.len() }
}
impl IsSeed for Seed {
    fn as_slice(&self) -> &[Element] { &self.elems }
}
impl IsSeed for Box<[Element]> {
    fn as_slice(&self) -> &[Element] { self }
}

pub type EncounteredMap = FxHashMap<Element, Vec<Seed>>;
pub type ElementBaseCacheMap = Vec<Option<Box<[Element]>>>;



thread_local! {
    // Pool of HashSets per thread
    static ALL_RESULTS_POOL: RefCell<Vec<Vec<u32>>> = const { RefCell::new(Vec::new()) };
}







#[derive(Default, Clone)]
pub struct DepthExplorerVars {
    pub lineage_elements: Vec<Element>,
    pub stop_after_depth: usize,
    pub exclude_depth1_elements: Vec<Element>,
    pub split_start: usize,
    pub split_start_msg: String,
    pub disable_depth_logs: bool,
}



struct DepthExplorerPrivateStructures {
    base_lineage_vec: Box<[Element]>,
    depth1: FxHashSet<u32>,
    base_lineage_depth1: FxHashSet<u32>,

    depth: usize,
    next_seed_set: DashSet<Seed>,
    encountered: EncounteredMap,
    
    element_base_cache: ElementBaseCacheMap,
    num_to_str_len: Box<[usize]>,

    start_time: Instant,
}

type RealRecipesResult = FxHashMap<Element, Vec<(Element, Element, Option<Element>)>>;






impl RecipesState {
    #[async_recursion]
    pub async fn depth_explorer_split_start(&mut self, de_vars: &DepthExplorerVars) -> EncounteredMap {
        if de_vars.split_start == 0 { return self.depth_explorer_start(de_vars).await; }
        let start_time = Instant::now();

        // calculate depth 1
        let mut initial_split_de_vars = de_vars.clone();
        initial_split_de_vars.stop_after_depth = 1;
        let initial_split_encountered = self.depth_explorer_start(&initial_split_de_vars).await;

        // iterate over every 1-step element
        let total_to_process = initial_split_encountered.len();
        let mut process_count = 0;

        let mut process_element = async |element: Element, excluded_depth1_elements: Vec<u32>| {
            // create new de_vars starting from every 1-step
            let mut new_de_vars = de_vars.clone();
            new_de_vars.lineage_elements.push(element);
            new_de_vars.stop_after_depth -= 1;
            new_de_vars.split_start -= 1;
            write!(new_de_vars.split_start_msg, "{}/{} {} > ",
                ({ process_count += 1; process_count }).to_string().purple(),
                total_to_process.to_string().purple(),
                self.num_to_str_fn(element).purple()
            ).unwrap();
            new_de_vars.exclude_depth1_elements = excluded_depth1_elements;
            new_de_vars.disable_depth_logs = true;

            println!("{}Time: {}",
                new_de_vars.split_start_msg,
                format!("{:?}", start_time.elapsed()).yellow(),
            );

            let mut new_encountered = if new_de_vars.split_start == 0
            { self.depth_explorer_start(&new_de_vars).await }
            else { self.depth_explorer_split_start(&new_de_vars).await };

            if !DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED {
                let element_ic = self.neal_case_map[element as usize];
                new_encountered = extend_encountered_seeds(new_encountered, element_ic);
            }
            new_encountered
        };

        
        // sequential processing
        let mut collected_encountereds = initial_split_encountered.clone();

        for element in initial_split_encountered.into_keys() /* .collect::<Vec<Element>>().into_iter().rev() */ {
            let element_encountered = process_element(element, initial_split_de_vars.exclude_depth1_elements.clone()).await;
            if !DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED {
                collected_encountereds = merge_encountered_maps(collected_encountereds, element_encountered);
            }
            initial_split_de_vars.exclude_depth1_elements.push(element);
        }




        if !de_vars.disable_depth_logs {
            println!("finished split depth explorer: {},  Elements: {}",
                format!("{:?}", start_time.elapsed()).red(),
                collected_encountereds.len().to_string().red()
            );
        }
        collected_encountereds
    }












    pub async fn depth_explorer_start(&mut self, de_vars: &DepthExplorerVars) -> EncounteredMap {
        let base_lineage_vec: FxHashSet<Element> = BASE_IDS.chain(de_vars.lineage_elements.iter().copied()).collect();
        let base_lineage_vec_ic: Box<[Element]> = base_lineage_vec.iter().map(|&x| self.neal_case_map[x as usize]).collect();
        
        // --- Private Structures ---
        let mut de_struc = DepthExplorerPrivateStructures {
            base_lineage_depth1: base_lineage_vec,
            base_lineage_vec: base_lineage_vec_ic,
            depth1: FxHashSet::default(),

            depth: 1,
            next_seed_set: DashSet::default(),
            encountered: FxHashMap::default(),

            element_base_cache: Vec::new(),
            num_to_str_len: Self::get_num_to_str_len(&self.num_to_str),

            start_time: Instant::now(),
        };


        // --- Depth 1 ---
        loop {
            let mut depth1 = Vec::new();
            self.all_combination_results(&de_struc.base_lineage_vec, &mut depth1, &de_struc, false);
            if !self.to_request_recipes.is_empty() {
                if DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED { self.mark_all_to_request_recipes_unknown(); }
                else { self.process_all_to_request_recipes("Depth 1").await; }
                continue;
            }
        
            let empty_seed = Seed::default();
            let empty_encountered_arc = Arc::new(EncounteredMap::default());
            for &x in depth1.iter().filter(|x| !de_vars.exclude_depth1_elements.contains(x)) {
                add_to_local_encountered(x, &empty_seed, &mut de_struc.encountered, &empty_encountered_arc);
            }

            let depth1_ic: Vec<Element> = depth1
                .into_iter()
                .map(|x| self.neal_case_map[x as usize])
                .filter(|&x| de_struc.num_to_str_len[x as usize] <= 30)
                .collect();

            de_struc.base_lineage_depth1.extend(depth1_ic.iter());

            de_struc.depth1 = depth1_ic.into_iter().filter(|x| !de_vars.exclude_depth1_elements.contains(x)).collect();
            de_struc.next_seed_set = de_struc.depth1.iter().map(|&x| {
                let mut seed = Seed::default();
                seed.add_element(x);
                seed
            }).collect();
            break;
        }
        


        


        
        // --- Main Loop ---
        while de_struc.depth < de_vars.stop_after_depth {

            // --- do all Element - Base combinations and cache them ---
            self.cache_all_element_base_results(&mut de_struc);


            let past_seed_set = de_struc.next_seed_set;

            let final_depth = de_struc.depth + 1 == de_vars.stop_after_depth;
            de_struc.next_seed_set = if final_depth { DashSet::default() }
            else { DashSet::with_capacity(past_seed_set.len() * DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS) };


            // --- Main Processing Loop ---
            let new_encountered = past_seed_set
                .par_iter()
                .fold(
                    EncounteredMap::default,
                    |mut local_encountered, seed| {
                        self.proccess_seed_logic(&seed, &de_struc, &mut local_encountered, final_depth);
                        local_encountered
                    }
                )
                .reduce_with(merge_encountered_maps)
                .unwrap_or_else(EncounteredMap::default);

            // -- Sequential Merge --
            de_struc.encountered = merge_encountered_maps(de_struc.encountered, new_encountered);
            
            
            if self.to_request_recipes.is_empty() {
                de_struc.depth += 1;
                de_struc.next_seed_set.shrink_to_fit();

                if !de_vars.disable_depth_logs {
                    println!("Depth {} complete!  Time: {},  Elements: {},  Seeds: {} -> {}",
                        de_struc.depth.to_string().yellow(),
                        format!("{:?}", de_struc.start_time.elapsed()).yellow(),
                        de_struc.encountered.len().to_string().yellow(),
                        past_seed_set.len().to_string().yellow(),
                        de_struc.next_seed_set.len().to_string().yellow(),
                    );
                }
            }
            else {
                println!("Depth {} paused. Requesting {} new recipes...", de_struc.depth + 1, self.to_request_recipes.len());
                de_struc.element_base_cache.clear();
                de_struc.next_seed_set = past_seed_set;
                if DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED {
                    self.mark_all_to_request_recipes_unknown();
                    break;
                }
                else { self.process_all_to_request_recipes(&format!("Depth {}", de_struc.depth + 1)).await; }
            }
        }

        de_struc.encountered
    }











    fn proccess_seed_logic(
        &self,
        seed: &Seed,
        de_struc: &DepthExplorerPrivateStructures,
        local_encountered: &mut EncounteredMap,
        final_depth: bool,
    ) {
        // all seed-seed and seed-base combinations
        let mut all_results = ALL_RESULTS_POOL.with(|pool_cell| {
            pool_cell.borrow_mut().pop().unwrap_or_else(|| {
                Vec::new()
            })
        });
        // clear before use
        all_results.clear();

        self.all_combination_results(&seed.elems, &mut all_results, de_struc, true);

        // for some reason its faster without deduplication...
        // all_results.sort_unstable();
        // all_results.dedup();



        if final_depth {
            if !DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED {
                for &result in &all_results {
                    add_to_local_encountered(result, seed, local_encountered, &de_struc.encountered);
                }
            }
        }
        else {
            let mut count_depth1s: i32 = 0;

            for result in &seed.elems {
                if de_struc.depth1.contains(result) { count_depth1s += 1; }
            }

            // if seed doesn't have too many depth1s already (crazy formula i know)
            if 3*(count_depth1s + 1) - 2*(de_struc.depth as i32) <= 4 {

                // extend seed with depth1 elements
                for d1 in &de_struc.depth1 {
                    if !seed.elems.contains(d1) {
                        let mut new_seed = seed.clone();
                        new_seed.add_element(*d1);

                        de_struc.next_seed_set.insert(new_seed);
                    }
                }
            }


            for &result in &all_results {
                add_to_local_encountered(result, seed, local_encountered, &de_struc.encountered);

                if de_struc.num_to_str_len[result as usize] > 30 { continue; }
            

                // eliminate seeds with too many depth1s
                // this was really difficult to come up with.
                // it definitely does not eliminate all useless seeds, but its the best i can do without storing recipes inside each seed
                // (count_depth1s - (2 * (depth + 1 - count_depth1s)) <= 2)
                // = (3*count_depth1s - 2*depth <= 4)

                // (count_depth1s + if depth1.contains(&result) {1} else {0}) = total depth1s in the full seed.
                let result_ic = self.neal_case_map[result as usize];

                if 3*(count_depth1s + (if de_struc.depth1.contains(&result_ic) {1} else {0})) - 2*(de_struc.depth as i32) <= 4
                    && !seed.elems.contains(&result_ic) {
                    
                    let mut new_seed = seed.clone();
                    new_seed.add_element(result_ic);
                    
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
        &self,
        input_seed: &[Element],
        results_vec: &mut Vec<Element>,
        de_struc: &DepthExplorerPrivateStructures,
        use_cache: bool
    ) {
        for (i, &ing1) in input_seed.iter().enumerate() {
            // all seed-seed combinations
            for &ing2 in input_seed.iter().take(i + 1) {
                let comb = sort_recipe_tuple((ing1, ing2));
                if let Some(&result) = self.recipes_ing.get(&comb) {
                    if result != NOTHING_ID && !de_struc.base_lineage_depth1.contains(&result) {
                        results_vec.push(result);
                    }
                }
                else { 
                    // comb does not exist, add it to the requests
                    self.to_request_recipes.insert(comb);
                }
            }

            if use_cache {
                let ing1_with_base_cache = de_struc.element_base_cache[ing1 as usize].as_ref().unwrap_or_else(|| panic!("{} not in cache", self.num_to_str_fn(ing1)));
                results_vec.extend(ing1_with_base_cache);
            }
        }
    }









    fn cache_all_element_base_results(&self, de_struc: &mut DepthExplorerPrivateStructures) {
        // let start_time = Instant::now();

        // resize element_base_cache vec to the max ID
        de_struc.element_base_cache.resize(self.neal_case_map.len(), None);


        for (&element, seeds) in &de_struc.encountered {
            if de_struc.num_to_str_len[element as usize] > 30 || seeds.first().unwrap().len() >= de_struc.depth { continue; }
            let neal_element = self.neal_case_map[element as usize];

            if de_struc.element_base_cache[neal_element as usize].is_none() {
                // cache results
                let mut cache_results = Vec::new();

                for &base_element in &de_struc.base_lineage_vec {
                    let comb = sort_recipe_tuple((neal_element, base_element));
                    if let Some(&result) = self.recipes_ing.get(&comb) {
                        if result != NOTHING_ID && !de_struc.base_lineage_depth1.contains(&result) && !cache_results.contains(&result) {
                            cache_results.push(result);
                        }
                    }
                    else { 
                        // comb does not exist, add it to the requests
                        self.to_request_recipes.insert(comb);
                    }
                }

                de_struc.element_base_cache[neal_element as usize] = Some(cache_results.into_boxed_slice());
            }
        }

        // println!("finished caching element - base: {:?}", start_time.elapsed());
    }













    pub fn get_encountered_entry<S: IsSeed>(
        &self,
        element: Element,
        seeds: &[S],
        initial_crafted: &FxHashSet<Element>,
        real_recipes_result: &RealRecipesResult
    ) -> String {

        let mut message = String::with_capacity(seeds.len() * seeds[0].len() * 7);
        if LINEAGES_FILE_COOL_JSON_MODE { write!(message, "{}: [", serde_json::to_string(&self.num_to_str_fn(element)).unwrap()).unwrap(); }
        else { write!(message, "{} - {}:", seeds[0].len() + 1, self.num_to_str_fn(element)).unwrap(); }

        for (i, seed) in seeds.iter().enumerate() {        

            let mut lineage: Vec<[Element; 3]> = Vec::with_capacity(seed.len() + 1);
            let mut to_craft: Vec<Element> = seed.as_slice().to_vec();
            let mut crafted: FxHashSet<Element> = initial_crafted.clone();
            let mut caps_map: FxHashMap<Element, Element> = FxHashMap::default();

            while !to_craft.is_empty() {
                let mut changes = false;

                to_craft.retain(|to_craft_element| {
                        real_recipes_result
                            .get(to_craft_element)
                            .expect("to_craft_element not in real_recipes_result")
                            .iter()
                            .find(|&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1))
                            .is_none_or(|recipe| {
                                crafted.insert(*to_craft_element);

                                if let Some(actual_caps_result) = recipe.2 {
                                    caps_map.insert(*to_craft_element, actual_caps_result);
                                }

                                lineage.push([
                                    *caps_map.get(&recipe.0).unwrap_or(&recipe.0),
                                    *caps_map.get(&recipe.1).unwrap_or(&recipe.1),
                                    *caps_map.get(to_craft_element).unwrap_or(to_craft_element),
                                ]);
                                changes = true;
                                false  // filter out
                            })  // keep
                    });
                assert!(changes, "could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, self.debug_elements(&to_craft));

            };

            let final_recipe = real_recipes_result
                .get(&element)
                .expect("element not in real_recipes_result")
                .iter()
                    // find with correct caps
                .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1) && rec.2.unwrap_or(element) == element)
                .expect("could not find a final_recipe...");

            lineage.push([
                *caps_map.get(&final_recipe.0).unwrap_or(&final_recipe.0),
                *caps_map.get(&final_recipe.1).unwrap_or(&final_recipe.1),
                element
            ]);


            let lineage_string = if LINEAGES_FILE_COOL_JSON_MODE { self.format_lineage_json_no_goals(&lineage) }
            else { self.format_lineage_no_goals(&lineage) };

            if i != 0 {
                if LINEAGES_FILE_COOL_JSON_MODE { write!(message, ",").unwrap(); }
                else { write!(message, " ...").unwrap(); }
            }
            if LINEAGES_FILE_COOL_JSON_MODE { write!(message, "{lineage_string}").unwrap(); }
            else { writeln!(message, "{lineage_string}").unwrap(); }
        };
        if LINEAGES_FILE_COOL_JSON_MODE { write!(message, "]").unwrap(); }
        else { write!(message, "\n\n").unwrap(); }

        message.shrink_to_fit();
        message
    }







    pub fn generate_lineages_file<S: IsSeed>(
        &self, lineage_elements: &[Element], max_depth: usize, encountered: &FxHashMap<Element, Vec<S>>
    ) -> io::Result<()> {
        // required for this function:
        let start_time = Instant::now();

        let mut real_recipes_result: RealRecipesResult = FxHashMap::with_capacity_and_hasher(
            self.neal_case_map.len(), FxBuildHasher
        );

        for (&(first, second), &result) in &self.recipes_ing {
            let f = self.neal_case_map[first as usize];
            let s = self.neal_case_map[second as usize];
            let r = self.neal_case_map[result as usize];
            if result == r {
                // if result is neal case
                real_recipes_result.entry(r).or_default().push((f, s, None));
            }
            else {
                // if result is not neal case
                real_recipes_result.entry(r).or_default().push((f, s, Some(result)));
                real_recipes_result.entry(result).or_default().push((f, s, None));
            }
        };
        println!("made recipes_result in {:?}", start_time.elapsed());


        let lineage_elements_str: Vec<String> = lineage_elements.iter().map(|x| self.num_to_str_fn(*x)).collect();
        let initial_crafted: FxHashSet<Element> = lineage_elements.iter().copied().collect();


        let start_time = Instant::now();

        let folder_name = "Lineages Files";
        let file_name = format!("{} Seed - {} Steps.{}",
            &self.num_to_str_fn(*lineage_elements.last().unwrap()),
            max_depth,
            if LINEAGES_FILE_COOL_JSON_MODE {"json"} else {"txt"}
        );

        let folder_path = PathBuf::from(folder_name);
        fs::create_dir_all(&folder_path)?;

        let full_path = folder_path.join(file_name);

        let file = File::create(full_path)?;
        let mut writer = BufWriter::new(file);

        // --- Parallel Processing ---
        let mut keyed_entries: Vec<(usize, u32, String)> = encountered
            .par_iter()
            .map(|(&element, seeds)| {

                let seed_len = seeds.first().unwrap().len();
                let formatted_string = self.get_encountered_entry(element, seeds, &initial_crafted, &real_recipes_result);
                (seed_len, element, formatted_string)
            })
            .collect();

        keyed_entries.par_sort_unstable_by_key(|(seed_len, element, ..)| (*seed_len, self.num_to_str_fn(*element)));


        let mut elements_per_depth_count = vec![0; max_depth]; // ---------------------------------------------------------
        for (seed_len, ..) in &keyed_entries {
            elements_per_depth_count[*seed_len] += 1;
        }





        // --- Writing ---
        if LINEAGES_FILE_COOL_JSON_MODE {
            writeln!(writer, "{{")?;
            writeln!(writer, "\"elements_ran\": [{}],\n", lineage_elements_str.iter().map(|x| serde_json::to_string(x).unwrap()).collect::<Vec<_>>().join(", "))?;

            writeln!(writer, "\"element_count_stats\": {{").unwrap();
            for (i, count) in elements_per_depth_count.into_iter().enumerate() {
                writeln!(writer, "    \"depth_{}\": {},", i + 1, count)?;
            }
            writeln!(writer, "    \"total\": {}", encountered.len())?;
            writeln!(writer, "}},\n\n\n").unwrap();

            writeln!(writer, "\"elements\": {{").unwrap();
            let mut iter = keyed_entries.iter().peekable();
            while let Some((_, _, entry_string)) = iter.next() {
                writer.write_all(entry_string.as_bytes())?;
                if iter.peek().is_some() { write!(writer, ",\n\n").unwrap();  }
                else { write!(writer, "\n\n").unwrap();  }
            }
            writeln!(writer, "}}").unwrap();


            
            write!(writer, "}}")?;
        }
        else {
            writeln!(writer, "TODO...  // {}\n\n\n", lineage_elements.len() - 4)?;

            for (i, count) in elements_per_depth_count.into_iter().enumerate() {
                writeln!(writer, "{} Steps - {} Elements", i + 1, count)?;
            }
            writeln!(writer, "Total Elements: {}\n\n\n\n", encountered.len())?;



            for (_, _, entry_string) in &keyed_entries {
                writer.write_all(entry_string.as_bytes())?;
            }
            // --- JSON Generation ---
            let json_map: FxHashMap<String, usize> = keyed_entries
                .into_iter()
                .map(|(seed_len, element, _formatted_string)| {
                    (self.num_to_str_fn(element), seed_len + 1)
                })
                .collect();

            let json_string = serde_json::to_string_pretty(&json_map).expect("JSON seriliaziation failed...");

            writer.write_all(b"\n")?;
            writer.write_all(json_string.as_bytes())?;
            writer.write_all(b"\n")?; 
        }




        println!("Generated Lineages File: {:?}", start_time.elapsed());

        Ok(())
    }
}





fn add_to_local_encountered(element: Element, seed: &Seed, local_map: &mut EncounteredMap, shared_read_encountered_map: &EncounteredMap) {
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
                entry.insert(vec![seed.clone()]);
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
                        for seed in local_seeds {
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



fn extend_encountered_seeds(mut encountered: EncounteredMap, add_element: Element) -> EncounteredMap {
    for original_seeds in encountered.values_mut() {
        for original_seed in original_seeds.iter_mut() {
            original_seed.add_element(add_element);
        }
    }
    encountered
}