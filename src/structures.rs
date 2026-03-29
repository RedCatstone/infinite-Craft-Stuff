use std::collections::BinaryHeap;
use std::fs::File;
use std::io::BufWriter;
use std::time::Instant;
use dashmap::DashSet;
use rayon::iter::IntoParallelRefIterator;
use rayon::iter::ParallelIterator;
use rayon::slice::ParallelSliceMut;
use rustc_hash::{FxHashMap, FxHashSet};
use std::cmp::Reverse;
use std::io;
use std::io::Write;

use crate::lineage::LineageStep;



pub const NOTHING_ID: Element = 0;
pub const UNKNOWN_ID: Element = 1;
pub const BASE_ELEMENTS: &[&str] = &["Water" /* id: 1 */, "Fire" /* id: 2 */, "Earth" /* id: 3 */, "Wind" /* id: 4 */];
pub const BASE_IDS: std::ops::RangeInclusive<Element> = 1..=(BASE_ELEMENTS.len() as u32);


pub fn is_base_element(element: Element) -> bool {
    BASE_IDS.contains(&element)
}





#[derive(Debug)]
pub struct RecipesState {
    pub num_to_str: Vec<String>,
    pub neal_case_map: Vec<u32>,
    pub recipes_ing: FxHashMap<(u32, u32), u32>,

    pub num_to_str_len: Box<[usize]>,

    pub to_request_recipes: DashSet<(u32, u32)>,
}
impl RecipesState {
    pub fn new() -> Self {
        let num_to_str: Vec<String> = ["Nothing", "=unknown="].into_iter().chain(BASE_ELEMENTS.iter().copied()).map(|x| x.to_string()).collect();
        Self {
            num_to_str_len: Self::get_num_to_str_len(&num_to_str),
            num_to_str,
            neal_case_map: [NOTHING_ID, UNKNOWN_ID].into_iter().chain(BASE_IDS).collect(),
            recipes_ing: FxHashMap::default(),
            to_request_recipes: DashSet::new()
        }
    }
}


pub type Element = u32;


pub type RecipesResultICMap = Vec<Vec<(Element, Element)>>;
pub type RecipesUsesICMap = Vec<Vec<(Element, Element)>>;
pub type ElementHeuristicMap = Vec<u64>;





pub fn start_case_unicode(input: &str) -> String {
    if input.is_empty() { return String::new(); }
    // Pre-allocate, but be aware capacity might be insufficient if chars expand (e.g., ß -> SS)
    let mut result = String::with_capacity(input.len());
    let mut capitalize_next = true;

    for c in input.chars() {
        if c.is_ascii_whitespace() {
            result.push(' ');
            capitalize_next = true;
        } else if capitalize_next {
            // to_uppercase returns an iterator
            for uppercased in c.to_uppercase() {
                result.push(uppercased);
            }
            capitalize_next = false;
        } else {
            // to_lowercase returns an iterator
            for lowercased in c.to_lowercase() {
                result.push(lowercased);
            }
            // capitalize_next remains false
        }
    }
    // Optional: Reclaim potentially over-allocated memory if string shrank or didn't grow much
    // result.shrink_to_fit();
    result
}


pub fn sort_recipe_tuple(tup: (u32, u32)) -> (u32, u32) {
    let (a, b) = tup;
    if a <= b { tup }
    else { (b, a) }
}




impl RecipesState {
    pub fn num_to_str_fn(&self, num: u32) -> String {
        self.num_to_str[num as usize].clone()
    }

    pub fn str_to_num_fn(&self, str: &str) -> u32 {
        self.num_to_str
            .iter()
            .position(|x| x == str)
            .expect("Element does not exist")
            as u32
    }

    pub fn debug_element(&self, element: Element) -> (Element, String) {
        (element, self.num_to_str_fn(element))
    }
    
    pub fn debug_element_vec(&self, element_vec: &[Element]) -> Vec<(Element, String)> {
        element_vec
            .iter()
            .map(|&element| self.debug_element(element))
            .collect()
    }
    
    pub fn debug_lineage_step_vec(&self, lineage_step_vec: &[LineageStep]) -> Vec<Vec<(Element, String)>> {
        lineage_step_vec
            .iter()
            .map(|step| step.iter().map(|&x| self.debug_element(x)).collect())
            .collect()
    }



    pub fn get_str_to_num_map(&self) -> FxHashMap<String, u32> {
        self.num_to_str.iter()
            .enumerate()
            .map(|(i, str)| (str.clone(), i as u32))
            .collect()
    }
    
    pub fn get_num_to_str_len(num_to_str: &[String]) -> Box<[usize]> {    
        num_to_str.iter()
            .map(|x| x.len())
            .collect()
    }

    pub fn variables_add_recipe(&mut self, first_str: &str, second_str: &str, result_str: &str, str_to_num: &mut FxHashMap<String, u32>) {
        let f = self.variables_add_element_str(first_str, str_to_num);
        let s = self.variables_add_element_str(second_str, str_to_num);
        let r = self.variables_add_element_str(result_str, str_to_num);
        self.recipes_ing.insert((f, s), r);
    }
    
    pub fn variables_add_element_str(&mut self, element_str: &str, str_to_num: &mut FxHashMap<String, u32>) -> Element {
        match str_to_num.get(element_str) {
            Some(&num) => {
                num
            }
            None => {
                let id = self.num_to_str.len() as u32;
                self.num_to_str.push(element_str.to_string());
                str_to_num.insert(element_str.to_string(), id);

                self.neal_case_map.push(0);  // immidiately push to reserve a spot
    
                let neal_str = start_case_unicode(element_str);
                let neal_id = self.variables_add_element_str(&neal_str, str_to_num);
                
                self.neal_case_map[id as usize] = neal_id;
    
                id
            }
        }
    }




    pub async fn rerequest_all_nothing_recipes(&mut self) {    
        for (recipe, r) in &self.recipes_ing {
            if *r == NOTHING_ID {
                self.to_request_recipes.insert(*recipe);
            }
        }
        self.process_all_to_request_recipes().await;
    }
    
    
    pub async fn request_all_unknown_recipes(&mut self) {
        for (recipe, r) in &self.recipes_ing {
            if *r == UNKNOWN_ID {
                self.to_request_recipes.insert(*recipe);
            }
        }
        self.process_all_to_request_recipes().await;
    }
    
    
    pub fn mark_all_to_request_recipes_unknown(&mut self) {
        let mut str_to_num = self.get_str_to_num_map();
        
        let to_request_recipes = std::mem::take(&mut self.to_request_recipes);
        println!("Marking {} recipes as '{UNKNOWN_ID}'...", to_request_recipes.len());
        
        for (f, s) in to_request_recipes.into_iter() {
            let first_str = &self.num_to_str[f as usize].clone();
            let second_str = &self.num_to_str[s as usize].clone();
    
            self.variables_add_recipe(first_str, second_str, &self.num_to_str_fn(UNKNOWN_ID), &mut str_to_num);
        }
    }





    pub fn find_and_write_dead_elements(&self, output_file_path: &str) -> io::Result<()> {
        println!("Finding dead elements...");
        let start_time = std::time::Instant::now();
    
        // 1. In a single parallel pass, identify "live" ingredients and all ingredients.
        let (live_elements, used_ingredients): (FxHashSet<Element>, FxHashSet<Element>) = self.recipes_ing
            .par_iter()
            .fold(
                || (FxHashSet::default(), FxHashSet::default()),
                |(mut live, mut used), (&(f, s), &r)| {
                    used.insert(f);
                    used.insert(s);
                    if r != NOTHING_ID {
                        live.insert(f);
                        live.insert(s);
                    }
                    (live, used)
                },
            )
            .reduce(
                || (FxHashSet::default(), FxHashSet::default()),
                |(mut live_a, mut used_a), (live_b, used_b)| {
                    live_a.extend(live_b);
                    used_a.extend(used_b);
                    (live_a, used_a)
                },
            );
    
        // 2. The dead elements are those used but not live.
        let mut dead_element_names: Vec<String> = used_ingredients
            .par_iter()
            .filter(|elem| !live_elements.contains(elem))
            .map(|&elem| self.num_to_str_fn(elem))
            .collect();
    
        println!("Found {} dead elements in {:?}.", dead_element_names.len(), start_time.elapsed());
    
        // 3. Sort and write the results to the file.
        println!("Writing dead elements to '{}'...", output_file_path);
        dead_element_names.par_sort_unstable(); // Parallel sort for speed
        
        let file = File::create(output_file_path)?;
        let mut writer = BufWriter::new(file);
        for name in dead_element_names {
            writeln!(writer, "{}", name)?;
        }
        println!("Finished writing.");
    
        Ok(())
    }








    pub fn string_lineage_results(&mut self, string_lineage: &str) -> Vec<u32> {
        let mut str_to_num = self.get_str_to_num_map();
        string_lineage
            .lines()
            .map(|line| {
                match line.rsplit_once('=') {
                    Some((_before, after)) => {
                        after.trim()
                    },
                    None => { line.trim() },
                }
            })
            .filter(|trimmed| !trimmed.is_empty())
            .map(|elem| self.variables_add_element_str(&start_case_unicode(elem), &mut str_to_num))
            .collect()
    }
    
    
    
    
    
    
    
    pub fn get_recipes_result_map(&self) -> RecipesResultICMap {
        let start_time = Instant::now();
    
        let mut recipes_result_ic_map = Vec::new();
        recipes_result_ic_map.resize(self.num_to_str.len(), Vec::new());
    
        for (&(f, s), &r) in &self.recipes_ing {
            let f_ic = self.neal_case_map[f as usize];
            let s_ic = self.neal_case_map[s as usize];
            let r_ic = self.neal_case_map[r as usize];
    
            recipes_result_ic_map[r_ic as usize].push((f_ic, s_ic));
        };
        println!("made recipes_result_ic_map in {:?}", start_time.elapsed());
    
        recipes_result_ic_map
    }
    
    
    
    
    pub fn get_recipes_uses_map(&self) -> RecipesUsesICMap {
        let start_time = Instant::now();
    
        let mut recipes_uses_map = Vec::new();
        recipes_uses_map.resize(self.num_to_str.len(), Vec::new());
    
        for (&(f, s), &r) in &self.recipes_ing {
            let f_ic = self.neal_case_map[f as usize];
            let s_ic = self.neal_case_map[s as usize];
            let r_ic = self.neal_case_map[r as usize];
    
            recipes_uses_map[f_ic as usize].push((s_ic, r_ic));
            recipes_uses_map[s_ic as usize].push((f_ic, r_ic));
        }
        println!("made recipes_uses_ic_map in {:?}", start_time.elapsed());
        recipes_uses_map
    }
    
    
    
    
    
    pub fn get_element_heuristic_map(&self, recipes_uses_map: &RecipesUsesICMap) -> ElementHeuristicMap {
        let start_time = Instant::now();
    
        let mut heuristic_map = Vec::new();
        heuristic_map.resize(self.num_to_str.len(), u64::MAX);
        for base_element in BASE_IDS {
            heuristic_map.insert(base_element as usize, 0);
        }
        update_heuristic_map(&mut heuristic_map, &Vec::from_iter(BASE_IDS), recipes_uses_map, u64::MAX);
    
        println!("made element_heuristic_map in {:?}", start_time.elapsed());
        heuristic_map
    }
}




pub fn update_heuristic_map(heuristic_map: &mut ElementHeuristicMap, start_elements: &[u32], recipes_uses_map: &RecipesUsesICMap, end: u64) {
    let mut heap: BinaryHeap<(Reverse<u64>, u32)> = BinaryHeap::new();

    for &element in start_elements.iter() {
        let heur = heuristic_map[element as usize];
        heap.push((Reverse(heur), element));
    }

    while let Some((Reverse(element_cost), element)) = heap.pop() {

        if element_cost > heuristic_map[element as usize] { continue; }

        for &(other, result) in &recipes_uses_map[element as usize] {

            let other_cost = if element == other { 0 }
            else { heuristic_map[other as usize] };

            let new_cost = element_cost.saturating_add(other_cost).saturating_add(1);

            let result_current_heuristic = &mut heuristic_map[result as usize]; // Get mutable ref
            if new_cost < *result_current_heuristic {
                *result_current_heuristic = new_cost;
                if new_cost <= end {
                    heap.push((Reverse(new_cost), result));
                }
            }
        }
    }
    // no return needed
}








// pub struct AutoLoadAndSaver {
//     save_logic: Arc<dyn Fn() + Send + Sync + 'static>,
//     interval_task: tokio::task::JoinHandle<()>,
// }

// impl AutoLoadAndSaver {
//     pub fn save_now(&self) {
//         (self.save_logic)();
//     }
//     pub fn abort(&self) {
//         self.interval_task.abort();
//     }
// }
// impl Drop for AutoLoadAndSaver {
//     fn drop(&mut self) {
//         self.save_now();
//         self.abort();
//     }
// }


// pub fn auto_load_and_save_recipes(interval: Duration, file_name: &str, format: recipe_loader::RecipeFileFormat) -> AutoLoadAndSaver {
//     recipe_loader::load(file_name, format)
//         .map_err(|err: io::Error| match err.kind() {
//             io::ErrorKind::NotFound => eprintln!("[WARNING] Failed to open recipe file '{}': {} Continuing without loading.", file_name, err),
//             _ => panic!("could not load recipes from {}: {}", file_name, err)
//         })
//         .ok();

//     let owned_file_name = file_name.to_string();
//     let arc_save_fn =  Arc::new(move || {
//         let owned_file_name_2 = owned_file_name.clone();
//         let result = panic::catch_unwind(move || {
//             // Execute the potentially panicking function
//             recipe_loader::save(&owned_file_name_2, format).unwrap();
//         });

//         if let Err(panic_payload) = result {
//             let msg = panic_payload.downcast_ref::<&str>().copied()
//                .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
//                .unwrap_or("Panic occurred with unknown payload type");
    
//             // Log the error instead of crashing
//             eprintln!("[WARNING] Auto-save function panicked: {}", msg);
//         }
//     });

//     AutoLoadAndSaver {
//         save_logic: arc_save_fn.clone(),
//         interval_task: tokio::spawn(async move {
//             let mut interval = time::interval(interval);
//             interval.tick().await;

//             loop {
//                 interval.tick().await;
//                 arc_save_fn.clone()();
//             }
//         }),
//     }
// }