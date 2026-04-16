use std::time::Instant;
use std::fmt::Write;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    old_depth_explorer::{DepthExplorerVars, Seed},
    structures::{Element, RecipesState, sort_recipe_tuple, ElementHeuristicMap, RecipesResultICMap, RecipesUsesICMap, is_base_element, update_heuristic_map, start_case_unicode, BASE_IDS}
};



pub type LineageStep = [Element; 3];

#[derive(Debug, Clone, Eq)]
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

// Custom hashing based on result of each step
// This needs to be consistent with PartialEq: if a.eq(b) then hash(a) == hash(b)
impl std::hash::Hash for Lineage {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let mut self_results: Vec<Element> = self.steps.iter().map(|[_, _, x]| *x).collect();
        self_results.sort_unstable();
        
        self_results.hash(state);
    }
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



impl RecipesState {
    pub fn format_lineage(&self, lineage: &Lineage) -> String {
        let mut output = String::new();

        for (i, &[f, s, r]) in lineage.steps.iter().enumerate() {
            write!(output,
                "\n{} + {} = {}",
                self.num_to_str_fn(f),
                self.num_to_str_fn(s),
                self.num_to_str_fn(r)
            ).unwrap();
            if lineage.goals.contains(&r) {
                write!(output, "  // {}", i + 1).unwrap();
            }
        }

        output
    }



    pub fn format_lineage_no_goals(&self, lineage: &[[u32; 3]]) -> String {
        let mut output = String::with_capacity(lineage.len() * 21);

        for &[f, s, r] in lineage {
            write!(output,
                "\n{} + {} = {}",
                self.num_to_str_fn(f),
                self.num_to_str_fn(s),
                self.num_to_str_fn(r)
            ).unwrap();
        }

        output.shrink_to_fit();
        output
    }

    pub fn format_lineage_json_no_goals(&self, lineage: &[[u32; 3]]) -> String {
        let mut output = String::with_capacity(lineage.len() * 21);
        write!(output, "[").unwrap();
        
        let mut iter = lineage.iter().peekable();
        while let Some(&[f, s, r]) = iter.next() {
            write!(output,
                "\n [{}, {}, {}]",
                serde_json::to_string(&self.num_to_str_fn(f)).unwrap(),
                serde_json::to_string(&self.num_to_str_fn(s)).unwrap(),
                serde_json::to_string(&self.num_to_str_fn(r)).unwrap(),
            ).unwrap();

            if iter.peek().is_some() {
                write!(output, ",").unwrap();
            }
        }

        write!(output, "\n]").unwrap();

        output.shrink_to_fit();
        output
    }









    pub fn string_lineage_to_lineage(&mut self, string_lineage: &str) -> Lineage {
        let mut str_to_num = self.get_str_to_num_map();

        let lineage: Vec<[u32; 3]> = string_lineage
            .lines()
            .map(|line| line.split_once(" //").map_or(line, |(x, _)| x).trim())
            .filter(|line| !line.is_empty())
            .map(|line| {
                let (first_second, result) = line.split_once(" = ").unwrap_or_else(|| panic!("no ' = ' found: {line}"));
                let (first, second) = first_second.split_once(" + ").unwrap_or_else(|| panic!("no ' + ' found: {line}"));
                assert!(!second.contains(" + "), "ambiguos ' + ': {line}");

                let mut f = self.variables_add_element_str(first.trim(), &mut str_to_num);
                let mut s = self.variables_add_element_str(second.trim(), &mut str_to_num);
                let r = self.variables_add_element_str(result.trim(), &mut str_to_num);
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
            let better = lowest_cost_opt.is_none_or(|lowest_cost| recipe_cost < lowest_cost);
            if better {
                if recipe_cost == 0 { return Some(*recipe); }
                lowest_cost_opt = Some(recipe_cost);
                best_recipe_opt = Some(recipe);
            }
        }
        best_recipe_opt.copied()
    }



    pub fn generate_lineage(
            &self,
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

            let best_recipe = Self::find_best_recipe(element, heuristic_map, recipes_result_map)
                .unwrap_or_else(|| panic!("{:?} does not have a working recipe", self.debug_element(element)));



            let check_first_element_first = match recalc {
                LineageRecalc::NoRecalc | LineageRecalc::Left => true,
                LineageRecalc::Right => false,
                LineageRecalc::Max => heuristic_map[best_recipe.0 as usize] > heuristic_map[best_recipe.1 as usize],
                LineageRecalc::Min => heuristic_map[best_recipe.0 as usize] < heuristic_map[best_recipe.1 as usize],
                LineageRecalc::Random => fastrand::bool(),
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

            if let Some(ing) = needed_ing {
                // add the element and one ingredient back to the queue
                element_queue.push(element);
                element_queue.push(ing);
            } else {
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

        Lineage {
            steps: lineage,
            goals: goals.to_vec(),
        }
    }




    pub fn generate_lineage_multiple_methods(
        &self,
        goals_str: &[&str],
        heuristic_map: &mut ElementHeuristicMap,
        recipes_result_map: &RecipesResultICMap,
        recipes_uses_map: &RecipesUsesICMap,
        print_every_lineage: bool,
    ) -> AltLineages {
        let goals: Vec<Element> = goals_str.iter().map(|&x| self.str_to_num_fn(&start_case_unicode(x)).unwrap()).collect();
    
        let lineage_methods: Vec<(&str, Box<dyn FnMut() -> Lineage + '_>)> = vec![
            ("Simple Generational", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::NoRecalc))),
            ("Recalc Left", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Left))),
            ("Recalc Right", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Right))),
            ("Recalc Max", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Max))),
            ("Recalc Min", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Min))),
            ("Recalc Random", Box::new(|| self.generate_lineage(&goals, &mut heuristic_map.clone(), recipes_result_map, recipes_uses_map, LineageRecalc::Random))),
        ];
    
        let mut alt_lineages = AltLineages::new(usize::MAX);
    
        for (method_name, mut lineage_func) in lineage_methods {
            let start_time = Instant::now();
            let mut lineage = lineage_func();
            let orig_len = lineage.steps.len();
            let orig_time = start_time.elapsed();
            lineage = self.remove_unneccessary(&lineage, &[], recipes_result_map);
    
            println!("({}) {} - {} Steps: {:?} -> {} Steps {:?}",
                goals_str.join(", "),
                method_name,
                orig_len,
                orig_time,
                lineage.steps.len(),
                start_time.elapsed() - orig_time,
            );
            if print_every_lineage {
                println!("{}", self.format_lineage(&lineage));
            }
            alt_lineages.add_lineage(lineage);
        }
        
        alt_lineages
    }



    pub async fn improve_lineage_depth_explorer(
        &mut self,
        input_lineage: Lineage,
        recipes_result_map: &RecipesResultICMap,
        stop_after_depth: usize,
        max_longer_than_shortest: usize
    ) -> AltLineages {
        let mut lineage_elements: Vec<Element> = input_lineage.steps.iter().map(|[_, _, x]| *x).collect();
        // if its only 1 goal remove it from the lineage_elements
        if input_lineage.goals.len() == 1 {
            let single_goal = input_lineage.goals[0];
            // Find the position of the single_goal in lineage_elements
            if let Some(index) = lineage_elements.iter().position(|&e| e == single_goal) {
                lineage_elements.remove(index);
            }
        }
    
        let mut alt_lineages = AltLineages::new(max_longer_than_shortest);
        alt_lineages.add_lineage(input_lineage);
    
        while let Some(lineage) = alt_lineages.to_process.pop() {
            let initial_crafted: FxHashSet<Element> = BASE_IDS
                .chain(lineage_elements.iter().map(|&x| self.neal_case_map[x as usize]))
                .collect();
    
            let mut shorter_found = false;
    
            for stopping_depth in 1..=stop_after_depth {
                let de_vars = DepthExplorerVars {
                    stop_after_depth: stopping_depth,
                    split_start: 0,
                    lineage_elements: lineage_elements.clone(),
                    ..Default::default()
                };
    
                let encountered = self.depth_explorer_split_start(&de_vars).await;
    
                for (element, seeds) in encountered {
                    for seed in seeds {
                        let mut seed_and_element = seed;
                        seed_and_element.elems.push(self.neal_case_map[element as usize]);
    
                        let seed_lineage = self.generate_lineage_from_results(seed_and_element, initial_crafted.clone(), recipes_result_map);
                        // println!("{:?} {:?}", debug_element_vec(&seed_and_element), debug_lineage_step_vec(&seed_lineage));
                        let lineage_shorter = self.remove_unneccessary(&lineage, &seed_lineage, recipes_result_map);
    
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
    
    
    
    
    
    
    
    
    
    pub fn generate_lineage_from_results(&self, seed: Seed, initial_crafted: FxHashSet<Element>, recipes_result_map: &RecipesResultICMap) -> Vec<LineageStep> {
        let mut lineage: Vec<LineageStep> = Vec::with_capacity(seed.len());
        let mut to_craft: Vec<Element> = seed.elems.iter().copied().collect();
        let mut crafted: FxHashSet<Element> = initial_crafted;
    
        while !to_craft.is_empty() {
            let mut changes = false;
    
            to_craft.retain(|&to_craft_element| {
                    recipes_result_map[to_craft_element as usize]
                        .iter()
                        .find(|&&rec| crafted.contains(&rec.0) && crafted.contains(&rec.1))
                        .is_none_or(|recipe| {
                            crafted.insert(to_craft_element);
                            
                            lineage.push([recipe.0, recipe.1, to_craft_element]);
                            changes = true;
                            false  // filter out
                        })
                });
            assert!(changes, "could not generate lineage...\n - lineage: {:?}\n - to_craft: {:?}", lineage, self.debug_elements(&to_craft));
        };
    
        lineage
    }



    pub fn correctly_order(&self, lineage: Lineage) -> Lineage {
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
    
            let recipe = result_ing_map.get(&element).unwrap_or_else(|| panic!("{:?} not in result_ing_map", self.debug_element(element)));
            let needed_ing =
                if !is_base_element(recipe.0) && !crafted.contains(&recipe.0) { Some(&recipe.0) }
                else if !is_base_element(recipe.1) && !crafted.contains(&recipe.1) { Some(&recipe.1) }
                else { None };
    
            if let Some(&ing) = needed_ing {
                element_queue.push(element);
                element_queue.push(ing);
            } else {
                // recipe can be added to lineage!
                new_lineage.push([recipe.0, recipe.1, element]);
                crafted.insert(element);
            }
        }
        Lineage {
            steps: new_lineage,
            goals: lineage.goals,
        }
    }




    pub fn remove_unneccessary(&self, lineage: &Lineage, add_recipes: &[[u32; 3]], recipes_result_map: &RecipesResultICMap) -> Lineage {
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
    
            for &r_use in local_used_map.get(r).expect("r not in used_map") {
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
    
        self.correctly_order(Lineage {
            steps: local_result_ing_map
                .into_iter()
                .map(|(r, ings)| [ings.0, ings.1, r])
                .collect(),
            goals: lineage.goals.clone(),
        })
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
    pub fn new(max_longer_than_shortest: usize) -> Self {
        Self {
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

    pub fn print_lineages_ordered(&self, state: &RecipesState) {
        for lineage in self.get_lineages_ordered().iter().rev() {
            println!("{}", state.format_lineage(lineage));
        }

    }
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