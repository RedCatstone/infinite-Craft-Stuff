use std::collections::BinaryHeap;
use std::time::Instant;
use dashmap::DashSet;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::sync::{OnceLock, RwLock, RwLockWriteGuard};
use tokio::time::{self, Duration};
use std::io::Error;


#[derive(Debug, Default)]
pub struct Variables {
    pub base_elements: [u32; 4],
    pub num_to_str: RwLock<Vec<String>>,
    pub neal_case_map: RwLock<Vec<u32>>,

    pub recipes_ing: RwLock<FxHashMap<(u32, u32), u32>>,
    pub recipes_result: RwLock<FxHashMap<u32, Vec<(u32, u32)>>>,
    pub recipes_uses: RwLock<FxHashMap<u32, Vec<(u32, u32)>>>,

    pub element_heuristic: RwLock<FxHashMap<u32, u64>>,

    pub to_request_recipes: DashSet<(u32, u32)>,
}

pub static VARIABLES: OnceLock<Variables> = OnceLock::new();





pub fn init_heuristic() {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let mut element_heuristic: RwLockWriteGuard<'_, FxHashMap<u32, u64>> = variables.element_heuristic.write().unwrap();

    update_recipes_uses();

    let empty_array: [u32; 0] = [];
    update_element_heuristics(&empty_array, &mut element_heuristic, u64::MAX);
}





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



pub fn auto_save_recipes(interval: Duration, save_func: impl Fn() -> Result<(), Error> + Send + Sync + 'static) {
    let mut recipe_count;
    {
        let variables = VARIABLES.get().expect("VARIABLES not initialized");
        let recipes_ing = variables.recipes_ing.read().unwrap();
        recipe_count = recipes_ing.len();
    }

    tokio::spawn(async move {
        let mut interval = time::interval(interval);
        loop {
            interval.tick().await;

            let bigger;
            {
                let variables = VARIABLES.get().expect("VARIABLES not initialized");
                let recipes_ing = variables.recipes_ing.read().unwrap();
                bigger = recipe_count < recipes_ing.len(); 
                recipe_count = recipes_ing.len();
            }

            if bigger { save_func().unwrap(); }
        }
    });
}








pub fn sort_recipe_tuple(tup: (u32, u32)) -> (u32, u32) {
    let (a, b) = tup;
    if a <= b { tup }
    else { (b, a) }
}


pub fn debug_print_recipe_tuple(tup: (u32, u32)) {
    println!("{:?} - {:?}", (tup.0, tup.1), (num_to_str_fn(tup.0), num_to_str_fn(tup.1)));
}

pub fn debug_print_element_vec(vec: &[u32]) -> Vec<(u32, String)> {
    let strings: Vec<(u32, String)> = vec
        .iter()
        .map(|&x| (x, num_to_str_fn(x)))
        .collect();

    println!("{:?}", strings);
    strings
}



pub fn string_lineage_results(lineage_text: &str) -> Vec<u32> {
    lineage_text
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
        .map(|elem| str_to_num_fn(elem))
        .collect()
}




pub fn update_recipes_result() {
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let mut recipes_result = variables.recipes_result.write().unwrap();
    let neal_case_map = variables.neal_case_map.read().unwrap();

    for (&(first, second), &result) in recipes_ing.iter() {
        let f = neal_case_map[first as usize];
        let s = neal_case_map[second as usize];
        let r = neal_case_map[result as usize];

        recipes_result.entry(r).or_default().push((f, s));
    };
    println!("updated recipes_result in {:?}", start_time.elapsed());
}




pub fn update_recipes_uses() {
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_result = variables.recipes_result.read().unwrap();
    let mut recipes_uses = variables.recipes_uses.write().unwrap();

    for (&r, recipes) in recipes_result.iter() {
        for &(f, s) in recipes {
            recipes_uses.entry(f).or_default().push((s, r));
            recipes_uses.entry(s).or_default().push((f, r));
        }
    }
    println!("updated recipes_uses in {:?}", start_time.elapsed());
}






pub fn update_element_heuristics(start_elements: &[u32], heuristic_map: &mut FxHashMap<u32, u64>, end: u64) {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let recipes_uses = variables.recipes_uses.read().unwrap();

    let mut heap: BinaryHeap<(Reverse<u64>, u32)> = BinaryHeap::new();

    let initial_elements = if !start_elements.is_empty() {start_elements} else { &variables.base_elements };

    for &elem in initial_elements {
        let heur = heuristic_map.get(&elem).copied().unwrap_or(0);
        if heuristic_map.insert(elem, heur).is_none() {
            heap.push((Reverse(heur), elem));
        }
    }

    while let Some((Reverse(element_cost), element)) = heap.pop() {

        if element_cost > heuristic_map.get(&element).copied().unwrap_or(u64::MAX) {
            continue;
        }

        if let Some(uses) = recipes_uses.get(&element) {
            for &(other, result) in uses {

                let other_cost = if element == other { 0 } else {
                    match heuristic_map.get(&other) {
                        Some(&x) => x,
                        None => continue,
                    }
                };

                let new_cost = element_cost.saturating_add(other_cost).saturating_add(1);
                if new_cost > end { continue; }

                let result_cost = heuristic_map.get(&result).copied().unwrap_or(u64::MAX);

                if new_cost < result_cost {
                    heuristic_map.insert(result, new_cost);
                    heap.push((Reverse(new_cost), result));
                }
            }
        }
    }
    // no return needed
}