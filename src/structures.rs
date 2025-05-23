use std::collections::BinaryHeap;
use std::time::Instant;
use dashmap::DashSet;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::panic;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tokio::time::{self, Duration};

use crate::lineage::LineageStep;
use crate::recipe_requestor::process_all_to_request_recipes;




pub static VARIABLES: OnceLock<Variables> = OnceLock::new();
pub const NOTHING_ID: Element = 0;
pub const BASE_ELEMENTS: &'static [&'static str] = &["Water" /* id: 1 */, "Fire" /* id: 2 */, "Earth" /* id: 3 */, "Wind" /* id: 4 */];
pub const BASE_IDS: std::ops::RangeInclusive<Element> = 1..=(BASE_ELEMENTS.len() as u32);

pub fn is_base_element(element: Element) -> bool {
    BASE_IDS.contains(&element)
}






#[derive(Debug, Default)]
pub struct Variables {
    pub num_to_str: RwLock<Vec<String>>,
    pub neal_case_map: RwLock<Vec<u32>>,
    pub recipes_ing: RwLock<FxHashMap<(u32, u32), u32>>,

    pub to_request_recipes: DashSet<(u32, u32)>,
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





pub fn num_to_str_fn(num: u32) -> String {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    variables.num_to_str.read().unwrap()
        .get(num as usize)
        .expect("Index out of bounds for NUM_TO_STR")
        .clone()
}


pub fn str_to_num_fn(str: &str) -> u32 {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    variables.num_to_str.read().unwrap()
        .iter()
        .position(|x| x == &str)
        .expect("Element does not exist")
        as u32
}







pub struct AutoSaver {
    save_logic: Arc<dyn Fn() + Send + Sync + 'static>,
    interval_task: tokio::task::JoinHandle<()>,
}

impl AutoSaver {
    pub fn save_now(&self) {
        (self.save_logic)();
    }
    pub fn abort(&self) {
        self.interval_task.abort();
    }
}
impl Drop for AutoSaver {
    fn drop(&mut self) {
        self.interval_task.abort();
    }
}


pub fn auto_save_recipes(interval: Duration, save_fn: impl Fn() + Send + Sync + 'static + panic::RefUnwindSafe) -> AutoSaver {
    let recipe_count_mutex = Arc::new(Mutex::new({
        let variables = VARIABLES.get().expect("VARIABLES not initialized");
        let recipes_ing = variables.recipes_ing.read().unwrap();
        recipes_ing.len()
    }));
    

    let arc_save_fn =  Arc::new(move || {
        let bigger;
        {
            let variables = VARIABLES.get().expect("VARIABLES not initialized");
            let recipes_ing = variables.recipes_ing.read().unwrap();
            let mut recipe_count_guard = recipe_count_mutex.lock().expect("lock poisoned");
            bigger = *recipe_count_guard < recipes_ing.len(); 
            *recipe_count_guard = recipes_ing.len();
        }

        if bigger { save_fn(); }
    });

    AutoSaver {
        save_logic: arc_save_fn.clone(),
        interval_task: tokio::spawn(async move {
            let mut interval = time::interval(interval);
            loop {
                interval.tick().await;

                let arc_save_fn_clone = arc_save_fn.clone();
                let result = panic::catch_unwind(move || {
                    // Execute the potentially panicking function
                    arc_save_fn_clone();
                });
                if let Err(panic_payload) = result {
                    let msg = panic_payload.downcast_ref::<&str>().copied()
                       .or_else(|| panic_payload.downcast_ref::<String>().map(|s| s.as_str()))
                       .unwrap_or("Panic occurred with unknown payload type");
    
                    // Log the error instead of crashing
                    eprintln!("Auto-save function panicked: {}", msg);
               }
            }
        }),
    }
}








pub fn sort_recipe_tuple(tup: (u32, u32)) -> (u32, u32) {
    let (a, b) = tup;
    if a <= b { tup }
    else { (b, a) }
}



pub fn debug_element(element: Element) -> (Element, String) {
    (element, num_to_str_fn(element))
}

pub fn debug_element_vec(element_vec: &[Element]) -> Vec<(Element, String)> {
    element_vec
        .iter()
        .map(|&element| debug_element(element))
        .collect()
}

pub fn debug_lineage_step_vec(lineage_step_vec: &Vec<LineageStep>) -> Vec<Vec<(Element, String)>> {
    lineage_step_vec
        .iter()
        .map(|step| step.iter().map(|&x| debug_element(x)).collect())
        .collect()
}



pub fn string_lineage_results(string_lineage: &str) -> Vec<u32> {
    let mut str_to_num = get_str_to_num_map();
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
        .map(|elem| variables_add_element_str(start_case_unicode(elem), &mut str_to_num))
        .collect()
}





pub fn variables_add_recipe(first_str: String, second_str: String, result_str: String, str_to_num: &mut FxHashMap<String, u32>) {
    let f = variables_add_element_str(first_str, str_to_num);
    let s = variables_add_element_str(second_str, str_to_num);
    let r = variables_add_element_str(result_str, str_to_num);

    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let mut recipes_ing = variables.recipes_ing.write().unwrap();

    recipes_ing.insert((f, s), r);
}



pub fn variables_add_element_str(element_str: String, str_to_num: &mut FxHashMap<String, u32>) -> Element {
    match str_to_num.get(&element_str) {
        Some(&num) => {
            num
        }
        None => {
            let variables = VARIABLES.get().expect("VARIABLES not initialized");
            let id;
            {
                let mut num_to_str = variables.num_to_str.write().unwrap();
                id = num_to_str.len() as u32;
                num_to_str.push(element_str.clone());
                str_to_num.insert(element_str.clone(), id);

                let mut neal_case_map = variables.neal_case_map.write().unwrap();
                neal_case_map.push(0);  // immidiately push to reserve a spot
            }

            let neal_str = start_case_unicode(&element_str.clone());
            let neal_id = variables_add_element_str(neal_str, str_to_num);
            
            let mut neal_case_map = variables.neal_case_map.write().unwrap();
            neal_case_map[id as usize] = neal_id;

            id
        }
    }
}





pub fn get_str_to_num_map() -> FxHashMap<String, u32> {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let num_to_str = variables.num_to_str.read().unwrap();
    num_to_str
        .iter()
        .enumerate()
        .map(|(i, str)| (str.clone(), i as u32))
        .collect()
}




pub fn get_num_to_str_len() -> Vec<usize> {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let num_to_str = variables.num_to_str.read().unwrap();

    num_to_str
        .iter()
        .map(|x| x.len())
        .collect()
}






pub fn get_recipes_result_map() -> RecipesResultICMap {
    let start_time = Instant::now();
    let variables = VARIABLES.get().expect("VARIABLES not initialized");

    let mut recipes_result_ic_map = Vec::new();
    recipes_result_ic_map.resize(variables.num_to_str.read().unwrap().len(), Vec::new());

    let recipes_ing = variables.recipes_ing.read().unwrap();
    let neal_case_map = variables.neal_case_map.read().unwrap();

    for (&(f, s), &r) in recipes_ing.iter() {
        let f_ic = neal_case_map[f as usize];
        let s_ic = neal_case_map[s as usize];
        let r_ic = neal_case_map[r as usize];

        recipes_result_ic_map[r_ic as usize].push((f_ic, s_ic));
    };
    println!("made recipes_result_ic_map in {:?}", start_time.elapsed());

    recipes_result_ic_map
}




pub fn get_recipes_uses_map() -> RecipesUsesICMap {
    let start_time = Instant::now();
    let variables = VARIABLES.get().expect("VARIABLES not initialized");

    let mut recipes_uses_map = Vec::new();
    recipes_uses_map.resize(variables.num_to_str.read().unwrap().len(), Vec::new());

    let recipes_ing = variables.recipes_ing.read().unwrap();
    let neal_case_map = variables.neal_case_map.read().unwrap();

    for (&(f, s), &r) in recipes_ing.iter() {
        let f_ic = neal_case_map[f as usize];
        let s_ic = neal_case_map[s as usize];
        let r_ic = neal_case_map[r as usize];

        recipes_uses_map[f_ic as usize].push((s_ic, r_ic));
        recipes_uses_map[s_ic as usize].push((f_ic, r_ic));
    }
    println!("made recipes_uses_ic_map in {:?}", start_time.elapsed());
    recipes_uses_map
}





pub fn get_element_heuristic_map(recipes_uses_map: &RecipesUsesICMap) -> ElementHeuristicMap {
    let start_time = Instant::now();
    let variables = VARIABLES.get().expect("VARIABLES not initialized");

    let mut heuristic_map = Vec::new();
    heuristic_map.resize(variables.num_to_str.read().unwrap().len(), u64::MAX);
    for base_element in BASE_IDS {
        heuristic_map.insert(base_element as usize, 0);
    }
    update_heuristic_map(&mut heuristic_map, &Vec::from_iter(BASE_IDS), &recipes_uses_map, u64::MAX);

    println!("made element_heuristic_map in {:?}", start_time.elapsed());
    heuristic_map
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








pub async fn rerequest_all_nothing_recipes() {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");

    for (recipe, r) in variables.recipes_ing.read().unwrap().iter() {
        if *r == NOTHING_ID {
            variables.to_request_recipes.insert(*recipe);
        }
    }
    process_all_to_request_recipes().await;
}