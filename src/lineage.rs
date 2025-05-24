use std::{fmt::{self, Write}, time::Instant};
use rand::Rng;
use rustc_hash::{FxHashMap, FxHashSet};
use async_recursion::async_recursion;

use crate::{depth_explorer::*, structures::*};



pub type LineageStep = [Element; 3];

#[derive(Debug, Clone)]
pub struct Lineage {
    pub steps: Vec<LineageStep>,
    pub goals: Vec<Element>,
}
// 2 Lineages are equal if each result is the same

impl PartialEq for Lineage {
    fn eq(&self, other: &Self) -> bool {
        if self.steps.len() != other.steps.len() {
            return false;
        }
        
        let mut self_results: Vec<Element> = self.steps.iter().map(|[_, _, x]| *x).collect();
        let mut other_results: Vec<Element> = other.steps.iter().map(|[_, _, x]| *x).collect();
        self_results.sort_unstable();
        other_results.sort_unstable();
        
        if self_results != other_results { return false; }
        true
    }
}
impl Eq for Lineage {} // Required if PartialEq is implemented

// Custom hashing based on result of each step
// This needs to be consistent with PartialEq: if a.eq(b) then hash(a) == hash(b)
impl std::hash::Hash for Lineage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut self_results: Vec<Element> = self.steps.iter().map(|[_, _, x]| *x).collect();
        self_results.sort_unstable();
        
        self_results.hash(state);
    }
}




pub fn format_lineage(lineage: &Lineage) -> String {
    let mut output = String::new();

    for (i, &[f, s, r]) in lineage.steps.iter().enumerate() {
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









pub fn string_lineage_to_lineage(string_lineage: &str) -> Lineage {
    let mut str_to_num = get_str_to_num_map();

    let lineage: Vec<[u32; 3]> = string_lineage
        .lines()
        .map(|line| line.split_once(" //").map_or(line, |(x, _)| x).trim())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let (first_second, result) = line.split_once(" = ").unwrap_or_else(|| panic!("no ' = ' found: {}", line));
            let (first, second) = first_second.split_once(" + ").unwrap_or_else(|| panic!("no ' + ' found: {}", line));
            if second.contains(" + ") { panic!("ambiguos ' + ': {}", line); }

            let mut f = variables_add_element_str(first.trim().to_string(), &mut str_to_num);
            let mut s = variables_add_element_str(second.trim().to_string(), &mut str_to_num);
            let r = variables_add_element_str(result.trim().to_string(), &mut str_to_num);
            (f, s) = sort_recipe_tuple((f, s));

            [f, s, r]
            
        })
        .collect();

    Lineage {
        // last element of lineage is the goal
        goals: lineage.last().map(|[_, _, r]| vec![*r]).unwrap_or_default(),
        steps: lineage,
    }
}






pub fn find_best_recipe(element: Element, heuristic_map: &ElementHeuristicMap, recipes_result_map: &RecipesResultICMap) -> Option<(u32, u32)> {
    let mut lowest_cost_opt = None;
    let mut best_recipe_opt = None;

    for recipe in &recipes_result_map[element as usize] {
        let &(f, s) = recipe;
        let f_cost = heuristic_map[f as usize];
        let s_cost = if f == s { 0 } else {
            heuristic_map[s as usize]
        };

        let recipe_cost = f_cost.saturating_add(s_cost);

        // if its the first recipe processed, or its better than the previous best
        let better = match lowest_cost_opt {
            None => true,
            Some(lowest_cost) => recipe_cost < lowest_cost
        };
        if better {
            if recipe_cost == 0 { return Some(*recipe); }
            lowest_cost_opt = Some(recipe_cost);
            best_recipe_opt = Some(recipe);
        }
    }
    best_recipe_opt.copied()
}




#[derive(PartialEq)]
pub enum LineageRecalc {
    NoRecalc,
    Left,
    Right,
    Max,
    Min,
    Random,
}


pub fn generate_lineage(
        goals: &[Element],
        heuristic_map: &mut ElementHeuristicMap,
        recipes_result_map: &RecipesResultICMap,
        recipes_uses_map: &RecipesUsesICMap,
        recalc: LineageRecalc
    ) -> Lineage {
    
    let mut element_queue = goals.to_vec();
    let mut crafted= FxHashSet::default();
    let mut lineage = Vec::new();


    while let Some(element) = element_queue.pop() {
        if crafted.contains(&element) { continue; }

        let best_recipe = find_best_recipe(element, heuristic_map, recipes_result_map)
            .unwrap_or_else(|| panic!("{:?} does not have a working recipe", debug_element(element)));

        

        let check_first_element_first = match recalc {
            LineageRecalc::NoRecalc => true,
            LineageRecalc::Left => true,
            LineageRecalc::Right => false,
            LineageRecalc::Max => heuristic_map[best_recipe.0 as usize] > heuristic_map[best_recipe.1 as usize],
            LineageRecalc::Min => heuristic_map[best_recipe.0 as usize] < heuristic_map[best_recipe.1 as usize],
            LineageRecalc::Random => rand::rng().random(),
        };

        let is_element_needed = |x| {
            if !is_base_element(x) && !crafted.contains(&x) { Some(x) }
            else { None }
        };

        let needed_ing = if check_first_element_first {
            is_element_needed(best_recipe.0).or_else(|| is_element_needed(best_recipe.1))
        } else {
            is_element_needed(best_recipe.1).or_else(|| is_element_needed(best_recipe.0))
        };

        match needed_ing {
            Some(ing) => {
                // add the element and one ingredient back to the queue
                element_queue.push(element);
                element_queue.push(ing);
            }
            None => {
                // element can be added to lineage!
                lineage.push([best_recipe.0, best_recipe.1, element]);
                crafted.insert(element);

                if recalc != LineageRecalc::NoRecalc && element_queue.len() > 1 {
                    heuristic_map[element as usize] = 0;
                    let max_goal_heuristic = goals.iter()
                        .map(|&goal_element| heuristic_map[goal_element as usize])
                        .max()
                        .unwrap();
                    update_heuristic_map(heuristic_map, &[element], recipes_uses_map, max_goal_heuristic);
                }
            }
        }
    }

    Lineage {
        steps: lineage,
        goals: goals.to_vec(),
    }
}













#[derive(Default)]
pub struct AltLineages {
    shortest: usize,
    max_longer_than_shortest: usize,
    all_lineages: FxHashSet<Lineage>,
    to_process: Vec<Lineage>,
}
impl AltLineages {
    pub fn new(max_longer_than_shortest: usize) -> AltLineages {
        AltLineages {
            max_longer_than_shortest,
            ..Default::default()
        }
    }

    pub fn add_lineage(&mut self, add_lineage: Lineage) -> bool {
        let add_lineage_len = add_lineage.steps.len();

        if self.all_lineages.is_empty() || add_lineage_len <= self.shortest + self.max_longer_than_shortest {
            // insert lineage
            if self.all_lineages.insert(add_lineage.clone()) {
                // if it is new
                self.to_process.push(add_lineage);
            }
        }

        if self.all_lineages.is_empty() || add_lineage_len < self.shortest {
            self.shortest = add_lineage_len;

            if self.max_longer_than_shortest != usize::MAX {
                self.all_lineages.retain(|x| x.steps.len() <= self.shortest + self.max_longer_than_shortest);
            }
            return true;
        }
        false
    }

    pub fn get_best(&self) -> Option<Lineage> {
        self.all_lineages.iter().min_by_key(|x| x.steps.len()).cloned()
    }

    pub fn get_lineages_ordered(&self) -> Vec<Lineage> {
        let mut lineages_vec: Vec<Lineage> = self.all_lineages.iter().cloned().collect();
        lineages_vec.sort_unstable_by_key(|x| x.steps.len());
        lineages_vec
    }

    pub fn print_lineages_ordered(&self) {
        for lineage in self.get_lineages_ordered().iter().rev() {
            println!("{}", format_lineage(&lineage));
        }

    }
}








pub fn generate_lineage_multiple_methods(
    goals_str: &[&str],
    heuristic_map: &mut ElementHeuristicMap,
    recipes_result_map: &RecipesResultICMap,
    recipes_uses_map: &RecipesUsesICMap,
    print_every_lineage: bool,
) -> AltLineages {
    let goals: Vec<Element> = goals_str.iter().map(|&x| str_to_num_fn(&start_case_unicode(x))).collect();

    let lineage_methods: Vec<(&str, Box<dyn FnMut() -> Lineage + '_>)> = vec![
        ("Simple Generational", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::NoRecalc))),
        ("Recalc Left", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Left))),
        ("Recalc Right", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Right))),
        ("Recalc Max", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Max))),
        ("Recalc Min", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Min))),
        ("Recalc Random", Box::new(|| generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Random))),
    ];

    let mut alt_lineages = AltLineages::new(usize::MAX);

    for (method_name, mut lineage_func) in lineage_methods {
        let start_time = Instant::now();
        let mut lineage = lineage_func();
        let orig_len = lineage.steps.len();
        let orig_time = start_time.elapsed();
        lineage = remove_unneccessary(&lineage, &[], recipes_result_map);

        println!("({}) {} - {} Steps: {:?} -> {} Steps {:?}",
            goals_str.join(", "),
            method_name,
            orig_len,
            orig_time,
            lineage.steps.len(),
            start_time.elapsed() - orig_time,
        );
        if print_every_lineage {
            println!("{}", format_lineage(&lineage));
        }
        alt_lineages.add_lineage(lineage);
    }
    
    alt_lineages
}









pub async fn improve_lineage_depth_explorer(input_lineage: Lineage, stop_after_depth: usize, max_longer_than_shortest: usize) -> AltLineages {
    let mut lineage_elements: Vec<Element> = input_lineage.steps.iter().map(|[_, _, x]| *x).collect();
    // if its only 1 goal remove it from the lineage_elements
    if input_lineage.goals.len() == 1 {
        let single_goal = input_lineage.goals[0];
        // Find the position of the single_goal in lineage_elements
        if let Some(index) = lineage_elements.iter().position(|&e| e == single_goal) {
            lineage_elements.remove(index);
        }
    }

    let recipes_result_map = get_recipes_result_map();
    let mut alt_lineages = AltLineages::new(max_longer_than_shortest);
    alt_lineages.add_lineage(input_lineage);

    while let Some(lineage) = alt_lineages.to_process.pop() {
        let initial_crafted: FxHashSet<Element> = BASE_IDS.chain(lineage_elements.iter().copied()).collect();

        let mut shorter_found = false;

        for stopping_depth in 1..=stop_after_depth {
            let de_vars = DepthExplorerVars {
                stop_after_depth: stopping_depth,
                split_start: 0,
                lineage_elements: lineage_elements.clone(),
                ..Default::default()
            };

            let encountered = depth_explorer_split_start(&de_vars).await;
            let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized");
            let neal_case_map = variables.neal_case_map.read().unwrap();

            for (element, seeds) in encountered.into_iter() {
                for seed in seeds.into_iter() {
                    let mut seed_and_element = seed;
                    seed_and_element.push(neal_case_map[element as usize]);

                    let seed_lineage = generate_lineage_from_results(seed, initial_crafted.clone(), &recipes_result_map);
                    println!("{:?}", debug_lineage_step_vec(&seed_lineage));
                    let lineage_shorter = remove_unneccessary(&lineage, &seed_lineage, &recipes_result_map);

                    if alt_lineages.add_lineage(lineage_shorter.clone()) {
                        println!("Found a {} Step", lineage_shorter.steps.len());
                        shorter_found = true;
                    }
                }
            }
            if shorter_found { break; }
        }
    }

    alt_lineages
}









pub fn generate_lineage_from_results(seed: Seed, initial_crafted: FxHashSet<Element>, recipes_result_map: &RecipesResultICMap) -> Vec<LineageStep> {
    let mut lineage: Vec<LineageStep> = Vec::with_capacity(seed.len() + 1);
    let mut to_craft: Vec<Element> = seed.iter().copied().collect();
    let mut crafted: FxHashSet<Element> = initial_crafted.clone();

    while !to_craft.is_empty() {
        let mut changes = false;

        to_craft = to_craft
            .iter()
            .filter(|&&to_craft_element| {
                if let Some(recipe) = recipes_result_map[to_craft_element as usize]
                    .iter()
                    .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1)) {
                    
                    crafted.insert(to_craft_element);
                    
                    lineage.push([recipe.0, recipe.1, to_craft_element]);
                    changes = true;
                    false  // filter out
                }
                else { true }  // keep
            })
            .copied()
            .collect();
        if !changes { panic!("could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, debug_element_vec(&to_craft)); }
    };

    lineage
}












pub fn remove_unneccessary(lineage: &Lineage, add_recipes: &[[u32; 3]], recipes_result_map: &RecipesResultICMap) -> Lineage {
    let mut local_result_ing_map: FxHashMap<Element, (Element, Element)> = lineage.steps
        .iter()
        .chain(add_recipes.iter())
        .map(|&recipe| (recipe[2], (recipe[0], recipe[1])))
        .collect();

    let mut local_used_map: FxHashMap<Element, FxHashSet<Element>> = lineage.steps
        .iter()
        .chain(add_recipes.iter())
        .fold(FxHashMap::default(), |mut acc_map, recipe| {
            if !is_base_element(recipe[0]) { acc_map.entry(recipe[0]).or_default().insert(recipe[2]); }
            if !is_base_element(recipe[1]) { acc_map.entry(recipe[1]).or_default().insert(recipe[2]); }
            acc_map.entry(recipe[2]).or_default();
            acc_map
        });



    for [_, _, r] in lineage.steps.iter().rev() {
        if lineage.goals.contains(r) { continue; }

        let blacklist = ru_get_blacklist(*r, &local_used_map);
        let mut changes = Vec::new();
        let mut removeable = true;

        for &r_use in local_used_map.get(r).expect("r not in used_map").iter() {
            if let Some(&replacement_recipe) = recipes_result_map[r_use as usize]
                .iter()
                .find(|(f, s)|
                    (is_base_element(*f) || (local_result_ing_map.contains_key(f) && !blacklist.contains(f)))
                 && (is_base_element(*s) || (local_result_ing_map.contains_key(s) && !blacklist.contains(s)))
            ) {
                // found a replacement_recipe!
                changes.push((r_use, replacement_recipe));
            }
            else {
                // couldn't find a replacement_recipe -> r is not removeable...
                removeable = false;
                break;
            }
        }
        if removeable {
            // remove r from the lineage
            ru_switch_recipe(*r, None, &mut local_used_map, &mut local_result_ing_map);

            for (change_r, change_ings) in changes {
                ru_switch_recipe(change_r, Some(change_ings), &mut local_used_map, &mut local_result_ing_map);
            }
        }
    }

    correctly_order(Lineage {
        steps: local_result_ing_map
            .into_iter()
            .map(|(r, ings)| [ings.0, ings.1, r])
            .collect(),
        goals: lineage.goals.clone(),
    })
}





fn ru_get_blacklist(element: Element, current_used_map: &FxHashMap<Element, FxHashSet<Element>>) -> FxHashSet<Element> {
    let mut blacklist_queue = vec![element];
    let mut blacklist = FxHashSet::from_iter(blacklist_queue.iter().copied());

    while let Some(cur_element) = blacklist_queue.pop() {
        let cur_element_uses = current_used_map.get(&cur_element).expect("cur_element not in current_used_map");
        for &black_use in cur_element_uses {
            if blacklist.insert(black_use) {
                // first time inserting
                blacklist_queue.push(black_use);
            }
        }
    }
    blacklist
}



fn ru_switch_recipe(
    result: Element,
    new_recipe_option: Option<(Element, Element)>,     // passing in None removes the recipe
    used_map: &mut FxHashMap<Element, FxHashSet<Element>>,
    res_map: &mut FxHashMap<Element, (Element, Element)>,
) {
    let (orig_f, orig_s) = res_map.get(&result).unwrap();
    if !is_base_element(*orig_f) { used_map.get_mut(orig_f).unwrap().remove(&result); }
    if !is_base_element(*orig_s) { used_map.get_mut(orig_s).unwrap().remove(&result); }

    match new_recipe_option {
        None => { res_map.remove(&result); },
        Some(new_recipe) => {
            res_map.insert(result, new_recipe);
            if !is_base_element(new_recipe.0) { used_map.entry(new_recipe.0).or_default().insert(result); }
            if !is_base_element(new_recipe.1) { used_map.entry(new_recipe.1).or_default().insert(result); }
        }
    }
}











pub fn correctly_order(lineage: Lineage) -> Lineage {
    // println!("correctly order got lineage:{}", format_lineage(&lineage));

    let result_ing_map: FxHashMap<u32, (u32, u32)> = lineage.steps
        .iter()
        .map(|recipe| (recipe[2], (recipe[0], recipe[1])))
        .collect();

    let mut crafted: FxHashSet<u32> = FxHashSet::default();
    let mut element_queue: Vec<u32> = lineage.goals.clone();
    let mut new_lineage: Vec<[u32; 3]> = Vec::new();
        

    while let Some(element) = element_queue.pop() {
        if crafted.contains(&element) { continue; }

        let recipe = result_ing_map.get(&element).unwrap_or_else(|| panic!("{:?} not in result_ing_map", debug_element(element)));
        let needed_ing =
            if !is_base_element(recipe.0) && !crafted.contains(&recipe.0) { Some(&recipe.0) }
            else if !is_base_element(recipe.1) && !crafted.contains(&recipe.1) { Some(&recipe.1) }
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
        steps: new_lineage,
        goals: lineage.goals,
    }
}