use std::fmt::Write;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::structures::*;

#[derive(Debug, Clone)]
pub struct Lineage {
    pub lineage: Vec<[u32; 3]>,
    pub goals: Vec<u32>,
}



pub fn format_lineage(lineage: Lineage) -> String {
    let mut output = String::with_capacity(lineage.lineage.len() * 21);

    for (i, &[f, s, r]) in lineage.lineage.iter().enumerate() {
        write!(output,
            "\n{} + {} = {}",
            num_to_str_fn(f),
            num_to_str_fn(s),
            num_to_str_fn(r)
        ).unwrap();
        if lineage.goals.contains(&r) {
             write!(output, "  // {}", i + 1).unwrap();
        }
    }

    output.shrink_to_fit();
    output
}



pub fn format_lineage_no_goals(lineage: Vec<[u32; 3]>) -> String {
    let mut output = String::with_capacity(lineage.len() * 21);

    for &[f, s, r] in lineage.iter() {
        write!(output,
            "\n{} + {} = {}",
            num_to_str_fn(f),
            num_to_str_fn(s),
            num_to_str_fn(r)
        ).unwrap();
    }

    output.shrink_to_fit();
    output
}






pub fn find_best_recipe(element: &u32, heuristic_map: &FxHashMap<u32, u64>) -> Option<(u32, u32)> {
    let recipes_vec = VARIABLES.get()
        .expect("VARIABLES not initialized")
        .recipes_result
        .read()
        .unwrap();
    
    let recipes = recipes_vec
        .get(element)
        .unwrap_or_else(|| panic!("{:?} does not exist in RECIPES_RESULT", num_to_str_fn(*element)));


    let mut lowest_cost = u64::MAX;
    let mut best_recipe: Option<&(u32, u32)> = None;

    for recipe in recipes {
        let (f, s) = recipe;
        let f_cost = heuristic_map.get(f).copied().unwrap_or(u64::MAX);  // not in RECIPES_RESULT
        let s_cost = if f == s { 0 } else {
            heuristic_map.get(s).copied().unwrap_or(u64::MAX)  // not in RECIPES_RESULT
        };

        let score = f_cost.saturating_add(s_cost);

        if score < lowest_cost {
            if score == 0 {
                return Some(*recipe);
            }
            lowest_cost = score;
            best_recipe = Some(recipe);
        }
    }
    best_recipe.copied()
}





pub fn generate_lineage(goals: &[u32], recalc: u8) -> Lineage {

    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let mut heuristic_map = variables.element_heuristic.write().expect("ELEMENT_HEURISTIC not initialized").clone();
    let base_elements = variables.base_elements;
    
    let mut element_queue = goals.to_vec();
    let mut crafted: FxHashSet<u32> = FxHashSet::default();
    let mut lineage = Vec::new();


    while let Some(element) = element_queue.pop() {

        if crafted.contains(&element) { continue; }

        let (best_f, best_s) = find_best_recipe(&element, &heuristic_map).unwrap_or_else(|| panic!("{:?} does not have a working recipe", element));

        let needed_ing =
            if !base_elements.contains(&best_f) && !crafted.contains(&best_f) { Some(best_f) }
            else if !base_elements.contains(&best_s) && !crafted.contains(&best_s) { Some(best_s) }
            else { None };

        match needed_ing {
            Some(ing) => {
                element_queue.push(element);
                element_queue.push(ing);
            }
            None => {
                // recipe can be added to lineage!
                lineage.push([best_f, best_s, element]);
                crafted.insert(element);

                if recalc != 0 && !element_queue.is_empty() {
                    heuristic_map.insert(element, 0);
                    update_element_heuristics(&[element], &mut heuristic_map, u64::MAX);
                }
            }
        }
    }


    Lineage {
        lineage,
        goals: Vec::from(goals),
    }
}











pub fn generate_lineage_from_results(results: &[u32], already_made: &[u32], variables: &Variables) -> Vec<[u32; 3]> {
    let base_elements = variables.base_elements;
    let recipes_result = variables.recipes_result.read().unwrap();
    let recipes_ing = variables.recipes_ing.read().unwrap();

    let mut lineage: Vec<[u32; 3]> = Vec::new();
    let mut to_craft: Vec<u32> = results.iter().copied().collect();
    let mut crafted: FxHashSet<u32> = base_elements.iter().chain(already_made).copied().collect();

    while !to_craft.is_empty() {
        let mut changes = false;

        to_craft = to_craft
            .iter()
            .filter(|&element| {
                if let Some(recipe) = recipes_result
                    .get(element)
                    .expect("element not in recipes_result")
                    .iter()
                    .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1)) {
                    
                    crafted.insert(*element);
                    
                    let actual_caps = recipes_ing.get(&&sort_recipe_tuple((recipe.0, recipe.1))).unwrap_or_else(|| panic!("{:?} not in recipes_ing", debug_print_recipe_tuple((recipe.0, recipe.1))));
                    lineage.push([recipe.0, recipe.1, *actual_caps]);
                    changes = true;
                    false  // filter out
                }
                else { true }  // keep
            })
            .copied()
            .collect();

        if !changes { panic!("could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, to_craft); }
    };

    lineage
}











pub fn remove_unneccessary(lineage: Lineage) -> Lineage {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let base_elements = variables.base_elements;
    let recipes_result = variables.recipes_result.read().unwrap();


    let mut result_ing_map: FxHashMap<u32, (u32, u32)> = lineage.lineage
        .iter().map(|recipe| (recipe[2], (recipe[0], recipe[1])))
        .collect();


    let mut used_map: FxHashMap<u32, FxHashSet<u32>> = lineage.lineage
        .iter().fold(FxHashMap::default(), |mut acc_map, recipe| {
            if !base_elements.contains(&recipe[0]) { acc_map.entry(recipe[0]).or_default().insert(recipe[2]); }
            if !base_elements.contains(&recipe[1]) { acc_map.entry(recipe[1]).or_default().insert(recipe[2]); }
            acc_map.entry(recipe[2]).or_default();
            acc_map
        });

    

    let get_blacklist = |element: u32, current_used_map: &FxHashMap<u32, FxHashSet<u32>>| {
        let mut blacklist_queue = vec![element];
        let mut blacklist = FxHashSet::from_iter(blacklist_queue.clone());

        while let Some(cur_element) = blacklist_queue.pop() {
            for black_use in current_used_map.get(&cur_element).expect("cur_element not in current_used_map") {
                if blacklist.insert(*black_use) {
                    blacklist_queue.push(*black_use);
                }
            }
        }
        blacklist
    };




    for [_, _, r] in lineage.lineage.iter().rev() {
        if lineage.goals.contains(r) { continue; }

        let blacklist: FxHashSet<u32> = get_blacklist(*r, &used_map);
        let mut changes: Vec<(u32, (u32, u32))> = Vec::new();
        let mut removeable = true;

        for r_use in used_map.get(r).expect("r not in used_map").iter() {
            let replacement_recipe = recipes_result.get(r_use).expect("r_use not in recipes_result")
                .iter().find(|&recipe|
                    (base_elements.contains(&recipe.0) || result_ing_map.contains_key(&recipe.0)) && !blacklist.contains(&recipe.0)
                 && (base_elements.contains(&recipe.1) || result_ing_map.contains_key(&recipe.1)) && !blacklist.contains(&recipe.1)
            );

            match replacement_recipe {
                None => {
                    removeable = false;
                    break;
                }
                Some(&recipe) => {
                    changes.push((*r_use, recipe));
                }
            }
        }
        if removeable {
            // switch recipe of r
            let original_recipe = result_ing_map.remove(r).expect("result not found");
            for x in [original_recipe.0, original_recipe.1].iter() {
                if !base_elements.contains(x) { used_map.get_mut(x).expect("x not found").remove(r); }
            }

            for (change_r, change_ings) in changes {
                let original_recipe = result_ing_map.get(&change_r).expect("result not found");
                for x in [original_recipe.0, original_recipe.1].iter() {
                    if !base_elements.contains(x) { used_map.get_mut(x).expect("x not found").remove(&change_r); }
                }
                result_ing_map.insert(change_r, change_ings);
                for x in [change_ings.0, change_ings.1].iter() {
                    if !base_elements.contains(x) { used_map.get_mut(x).expect("x not found").insert(change_r); }
                }
            }
        }
    }

    correctly_order(Lineage {
        lineage: result_ing_map.iter().map(|(&r, &ings)| [ings.0, ings.1, r]).collect(),
        goals: lineage.goals
    })
}





pub fn correctly_order(lineage: Lineage) -> Lineage {
    let variables = VARIABLES.get().expect("VARIABLES not initialized");
    let base_elements = variables.base_elements;

    let result_ing_map: FxHashMap<u32, (u32, u32)> = lineage.lineage
        .iter().map(|recipe| (recipe[2], (recipe[0], recipe[1])))
        .collect();

    let mut crafted: FxHashSet<u32> = FxHashSet::default();
    let mut element_queue: Vec<u32> = lineage.goals.clone();
    let mut new_lineage: Vec<[u32; 3]> = Vec::new();
        

    while let Some(element) = element_queue.pop() {
        if crafted.contains(&element) { continue; }

        let recipe = result_ing_map.get(&element).expect("element not in result_ing_map");
        let needed_ing =
            if !base_elements.contains(&recipe.0) && !crafted.contains(&recipe.0) { Some(&recipe.0) }
            else if !base_elements.contains(&recipe.1) && !crafted.contains(&recipe.1) { Some(&recipe.1) }
            else { None };

        match needed_ing {
            Some(&ing) => {
                element_queue.push(element);
                element_queue.push(ing);
            }
            None => {
                // recipe can be added to lineage!
                new_lineage.push([recipe.0, recipe.1, element]);
                crafted.insert(element);
            }
        }
    }
    Lineage {
        lineage: new_lineage,
        goals: lineage.goals,
    }
}