use std::{collections::hash_map, fmt::Debug, time::Instant};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use tinyvec::ArrayVec;
use colored::Colorize;

use crate::{DEPTH_EXPLORER_MAX_STEPS, structures::{Element, NOTHING_ID, RecipesState, sort_recipe_tuple}};


/// This Algorithm generates all n-step elements starting from some ``base_elements``.
/// 
/// # How it works:
/// the way to make this kind of dfs algorithm fast is by trying to dedupe gamestates/seeds.
/// previously i did this by using a ginourmous ``HashSet`` that collects all gamestates and eats insane amounts of RAM.
/// 
/// This algorithm does actually perfectly deduplicate gamestates, simply by being smart.\
/// It crafts elements in Layers. Layer 0 is the ``base_element`` layer.\
/// Layer 1 is all elements you can craft from just base elements. (excluding ``base_elements``)\
/// Layer 2 is all elements you can craft from L0 and L1. (exlcuding elements you could have already crafted from L0)\
/// Layer N is all elements you can craft from previous layers (excluding elements you could have already crafted in previous-1 layers)\
/// and so on...
/// 
/// when it has all elements for one layer it adds them to the current gamestate in subsets.
/// example, the Layer 1 items are `[Plant, Lava, Dust]`, it will add `[Plant]` and calculate future layers, then
/// it will add `[Lava]` and calculate, then `[Dust], [Plant, Lava], [Plant, Dust], [Lava, Dust]` and finally `[Plant, Lava, Dust]`
/// we can optimize this by only allowing subsets for example up to length 3 if we only have 5 steps left. (explained later in the code)
/// 
/// if an element requires all of `[Plant, Lava, Dust]`, it gives it upfront, just so it can fully ban those elements for future layers.
/// this also means that processing a layer only requires combining the current layer elements with all previous elements. SPEEEED
/// 
/// for now (26.3.2026) its able to process depth 11 (156992 elements) in 144 seconds (multi-threaded)
/// now (27.3.2026) its 113 sec

#[derive(Clone)]
pub struct LayerExplorer<'a> {
    recipes: &'a RecipesState,
    encountered: EncounteredElements,

    layers: Vec<LayerData>,
    /// this includes all elements that can be crafted from all layers up until the current one.
    /// So if we can craft [Plant, Dust, Wave] at layer 1, then those elements are all banned at layer 2 and onwards.
    /// all of these elements will end up in subsets of layer 1 later anyways, so banning them is fine.
    banned_elems: Vec<bool>,
    /// the elements used in the current path. (including the base elements)
    curr_steps: Vec<Element>,
    max_steps: usize,

    // caching didn't end up being faster
    // cache: Vec<ArrayVec<[Element; LAYER_BASE_LENGTH + LAYER_STEPS - 2]>>,

    temp_results: Vec<Element>,
}

#[derive(Clone)]
pub struct LayerData {
    subset_iter: SubsetIter,
    start_idx: usize,
}


impl LayerExplorer<'_> {
    pub async fn start_step_by_step_with_requests(
        recipes: &mut RecipesState, base_elements: &[Element], max_steps: usize, multi_thread: bool, generate_lineages_file: bool
    ) -> EncounteredElements {
        let start_time = Instant::now();

        for i in 1..=max_steps {
            let encountered = Self::start(recipes, base_elements, i, multi_thread, false);
            if generate_lineages_file {
                recipes.generate_lineages_file(base_elements, max_steps, &encountered.elements)
                    .unwrap_or_else(|e| eprintln!("could not generate Lineages File... {e}"));
            }

            println!("Finished processing {i}-step elements ({}). To-request: {}",
                format!("{:?}", start_time.elapsed()).yellow(),
                recipes.to_request_recipes.len()
            );
            if !recipes.to_request_recipes.is_empty() {
                recipes.process_all_to_request_recipes(&format!("{i}-step")).await;
            } else if i == max_steps {
                // we can return early
                return encountered;
            }
        }
        Self::start(recipes, base_elements, max_steps, multi_thread, generate_lineages_file)
    }

    pub fn start(
        recipes: &RecipesState, base_elements: &[Element], max_steps: usize, multi_thread: bool, generate_lineages_file: bool
    ) -> EncounteredElements {
        let start_time = Instant::now();

        let neal_base_elements: Vec<Element> = base_elements.iter()
            .map(|&x| recipes.neal_case_map[x as usize])
            .collect();

        // start with the base_layer
        let base_element_layer = LayerData {
            subset_iter: SubsetIter::default(),  // this iter is done already.
            start_idx: 0
        };

        let mut le = LayerExplorer {
            recipes,
            encountered: EncounteredElements::default(),
            layers: vec![base_element_layer],
            banned_elems: vec![false; recipes.num_to_str.len()],
            curr_steps: neal_base_elements.clone(),
            max_steps: max_steps + base_elements.len(),
            temp_results: Vec::new(),
        };

        // mark starting base_elements as banned
        for elem in neal_base_elements {
            le.banned_elems[elem as usize] = true;
            // le.cache[elem as usize] = [].into();
            
        }
        // ban anything > 30 chars (dead elements)
        for (i, b) in le.banned_elems.iter_mut().enumerate() {
            if recipes.num_to_str[i].len() > 30 {
                *b = true;
            }
        }

        let final_encountered = if multi_thread {
            le.all_results_and_push_new_layer();

            let last_layer = le.layers.last_mut().unwrap();
            // extract all subsets (and finish the SubsetIter)
            let all_subsets: Vec<_> = (&mut last_layer.subset_iter).collect();

            all_subsets.into_par_iter()
                .fold(
                    || le.encountered.clone(),
                    |thread_encountered, start_subset| {
                        let mut thread_le = le.clone();
                        thread_le.encountered = thread_encountered;
                        thread_le.curr_steps.extend(start_subset);

                        thread_le.enter_main_loop();
                        thread_le.encountered
                    }
                )
                .reduce_with(EncounteredElements::merge_with)
                .unwrap_or_else(|| le.encountered.clone())
        } else {
            le.enter_main_loop();
            le.encountered
        };

        
        println!("Finished Layer Explorer! ({}) - Elements in {max_steps}-step: {} - to_request: {} - base_elements: {:?}",
            format!("{:?}", start_time.elapsed()).yellow(),
            final_encountered.len().to_string().purple(),
            recipes.to_request_recipes.len().to_string().green(),
            recipes.num_to_strs_fn(base_elements),
        );
        if generate_lineages_file {
            recipes.generate_lineages_file(base_elements, max_steps, &final_encountered.elements)
                .unwrap_or_else(|e| eprintln!("could not generate Lineages File... {e}"));
        }
        final_encountered
    }



    fn enter_main_loop(&mut self) {
        'main: loop {
            self.all_results_and_push_new_layer();

            // now advance the iter, if its done, remove the layer and repeat.
            while let Some(top_layer) = self.layers.last_mut() {
                // pop the old subset
                self.curr_steps.truncate(top_layer.start_idx);

                if let Some(sub) = top_layer.subset_iter.next() {
                    // and add the new one
                    self.curr_steps.extend(sub);
                    debug_assert!(self.curr_steps.len() < self.max_steps);
                    continue 'main;
                } else {
                    // layer is fully done, pop it and unban elements
                    for to_remove in self.layers.pop().unwrap().subset_iter.elements {
                        self.banned_elems[to_remove as usize] = false;
                    }
                }
            }

            // if we end up here, it means that we popped all the layers
            // => we are done!
            return;
        }
    }


    fn all_results_and_push_new_layer(&mut self) {
        let top_layer = self.layers.last().unwrap();
        let seed = match self.layers.get(1) {
            Some(x) => &self.curr_steps[x.start_idx..],
            None => &[]
        };
        let is_final_step = self.max_steps - self.curr_steps.len() == 1;

        macro_rules! process_result {
            ($result:expr) => {
                let neal_result = self.recipes.neal_case_map[$result as usize];

                if neal_result != NOTHING_ID {
                    // add it to encountered before extra checks
                    self.encountered.add_element($result, seed);

                    // extra checks for results-push
                    // add an element for processing only if there is more than 1 step left AND its not banned
                    if !is_final_step && !self.banned_elems[neal_result as usize] {
                        // ban it from other iterations in these for loops
                        // AND keep it banned for the next layer already (i can't believe that this works so nicely)
                        self.banned_elems[neal_result as usize] = true;
                        self.temp_results.push(neal_result);
                    }
                }
            };
        }
        
        // ing1 is one of the elements in the current top layer.
        for (i, &ing1) in self.curr_steps[top_layer.start_idx..].iter().enumerate() {

            // combine ing1 with all curr_steps which includes
            // all base_elements, all elements from past layers, and the current layer
            // (only up to itself there for not processing the same recipes twice)
            for &ing2 in &self.curr_steps[..=top_layer.start_idx + i] {
                let comb = sort_recipe_tuple((ing1, ing2));
                if let Some(&result) = self.recipes.recipes_ing.get(&comb) {
                    process_result!(result);
                } else { 
                    // comb does not exist, add it to the requests
                    self.recipes.to_request_recipes.insert(comb);
                }
            }
        }

        if !self.temp_results.is_empty() {
            // this should limit elements generated on this layer, based on how many steps are left.
            //
            // lets say we have 2 steps left, then we can generate 1 element on this layer.
            // 3 steps -> 2 elements.
            // 5 steps -> 3 elements.
            // 7 steps -> 4 elements.
            // 9 steps -> 5 elements.
            // 1 steps -> 0 elements.
            let max_subset_len = (self.max_steps - self.curr_steps.len()).div_ceil(2);

            self.layers.push(LayerData {
                start_idx: self.curr_steps.len(),
                subset_iter: SubsetIter::new(self.temp_results.clone().into_boxed_slice(), max_subset_len)
            });
            self.temp_results.clear();
        }
    }
}







#[derive(Clone, Default)]
pub struct SubsetIter {
    elements: Box<[Element]>,
    max_len: usize,
    curr_indices: Vec<usize>,
}

impl SubsetIter {
    /// `SubsetIter::new(vec![1, 2, 3], 3)`
    /// -> `[1], [2], [3], [4], [1, 2], [1, 3], [1, 4], [2, 3], [2, 4], [3, 4], [1, 2, 3], [1, 2, 4], [1, 3, 4], [2, 3, 4]`
    pub fn new(elements: Box<[Element]>, max_len: usize) -> Self {
        let max_len = max_len.min(elements.len());
        Self { elements, max_len, curr_indices: Vec::with_capacity(max_len) }
    }

    #[inline(always)]
    fn current_subset(&self) -> ArrayVec<[Element; DEPTH_EXPLORER_MAX_STEPS.div_ceil(2)]> {
        self.curr_indices.iter().map(|&i| self.elements[i]).collect()
    }
}

impl Iterator for SubsetIter {
    type Item = ArrayVec<[Element; DEPTH_EXPLORER_MAX_STEPS.div_ceil(2)]>;

    fn next(&mut self) -> Option<Self::Item> {
        // Check if it exceeded the maximum allowed length
        if self.curr_indices.len() > self.max_len {
            return None;
        }

        // Try to advance the indices for the curr_len
        for i in (0..self.curr_indices.len()).rev() {
            if self.curr_indices[i] < self.elements.len() - self.curr_indices.len() + i {
                self.curr_indices[i] += 1;
                for j in i + 1..self.curr_indices.len() {
                    self.curr_indices[j] = self.curr_indices[j - 1] + 1;
                }
                return Some(self.current_subset());
            }
        }

        // If it couldn't advance, it finished curr_len. Move to `curr_len + 1`
        let new_len = self.curr_indices.len() + 1;
        if new_len > self.max_len {
            None
        } else {
            // Re-initialize indices for the new subset length
            self.curr_indices.clear();
            self.curr_indices.extend(0..new_len);
            Some(self.current_subset())
        }
    }
}








#[derive(Debug, Default, Clone)]
pub struct EncounteredElements {
    pub elements: FxHashMap<Element, Vec<Box<[Element]>>>
}

impl EncounteredElements {
    pub fn add_element(&mut self, elem: Element, seed: &[Element]) {
        match self.elements.entry(elem) {
            hash_map::Entry::Occupied(mut occ) => {
                let existing = occ.get_mut();
                match seed.len().cmp(&existing[0].len()) {
                    std::cmp::Ordering::Less => {
                        // new seed is shorter, collect it
                        existing.clear();
                        existing.push(seed.into());
                    },
                    std::cmp::Ordering::Equal => {
                        // equal length, add it
                        if existing.iter().all(|s| **s != *seed) {
                            existing.push(seed.into());
                        }
                    },
                    std::cmp::Ordering::Greater => { /* new seed is longer, do nothing */ },
                }
            }
            hash_map::Entry::Vacant(vac) => {
                vac.insert(vec![seed.into()]);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.elements.len()
    }


    pub fn merge_with(mut self, mut other_map: Self) -> Self {
        if other_map.len() > self.len() {
            (self, other_map) = (other_map, self);
        }
    
        for (other_element, other_seeds) in other_map.elements {
            match self.elements.entry(other_element) {
                hash_map::Entry::Occupied(mut entry) => {
                    let main_seeds = entry.get_mut();
                    let main_len = main_seeds.first().unwrap().len(); // Assumes non-empty
                    let other_len = other_seeds.first().unwrap().len();
    
                    match other_len.cmp(&main_len) {
                        std::cmp::Ordering::Less => {
                            // Local map's seeds are shorter, replace main's
                            *main_seeds = other_seeds;
                        }
                        std::cmp::Ordering::Equal => {
                            // Same length, add seeds from local map if not already present
                            for other_seed in other_seeds {
                                if !main_seeds.contains(&other_seed) {
                                    main_seeds.push(other_seed);
                                }
                            }
                        }
                        std::cmp::Ordering::Greater => {
                            // Main map's seeds are shorter, do nothing with this element
                        }
                    }
                }
                hash_map::Entry::Vacant(entry) => {
                    entry.insert(other_seeds);
                }
            }
        }
        self
    }
}