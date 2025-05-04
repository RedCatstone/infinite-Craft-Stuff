use std::collections::BinaryHeap;
use std::time::Instant;
use dashmap::DashSet;
use rustc_hash::FxHashMap;
use std::cmp::Reverse;
use std::panic;
use std::sync::{Arc, Mutex, OnceLock, RwLock, RwLockWriteGuard};
use tokio::time::{self, Duration};


#[derive(Debug, Default)]
pub struct Variables {
    pub base_elements: [u32; 4],
    pub num_to_str: RwLock<Vec<String>>,
    pub neal_case_map: RwLock<Vec<u32>>,
    pub recipes_ing: RwLock<FxHashMap<(u32, u32), u32>>,

    pub to_request_recipes: DashSet<(u32, u32)>,

    // not always there
    pub recipes_result: RwLock<FxHashMap<u32, Vec<(u32, u32)>>>,
    pub recipes_uses: RwLock<FxHashMap<u32, Vec<(u32, u32)>>>,
    pub element_heuristic: RwLock<FxHashMap<u32, u64>>,

}

pub static VARIABLES: OnceLock<Variables> = OnceLock::new();



pub type Element = u32;

#[derive(Debug, Clone)]
pub struct Lineage {
    pub lineage: Vec<[Element; 3]>,
    pub goals: Vec<Element>,
}





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