use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::io::{self, BufWriter, BufReader, Write, Read, BufRead};
use std::fs::{self, File};
use rustc_hash::{FxHashMap, FxHashSet};
use rayon::prelude::*;

use libdeflater::{CompressionLvl, Compressor, Decompressor};

use crate::{LINEAGES_FILE_COOL_JSON_MODE, RECIPE_FILES_FOLDER, structures::*};




#[derive(Debug, Copy, Clone)]
pub enum RecipeFileFormat {
    ICSaveFile,
    JSONRecipesNum,
    JSONOldDepthExplorerRecipes
}

#[derive(Deserialize, Serialize, Default)]
struct RecipesNum {
    #[serde(default)]
    #[serde(alias = "numToStr")]
    num_to_str: Vec<String>,

    #[serde(default)]
    recipes: FxHashMap<u32, FxHashMap<u32, u32>>,
}

#[derive(Deserialize, Serialize)]
struct RecipesGzip {
    name: String,
    version: String,
    created: u128, // Milliseconds since UNIX epoch
    updated: u128, // Milliseconds since UNIX epoch
    instances: Vec<serde_json::Value>,
    items: Vec<RecipesGzipItemData>,
}
#[derive(Deserialize, Serialize)]
struct RecipesGzipItemData {
    id: u32,
    text: String,
    #[serde(default)]
    recipes: Vec<(u32, u32)>,
}

#[derive(Deserialize, Debug)]
struct CoolJsonLineagesFile {
    elements: FxHashMap<String, Vec<Vec<Vec<String>>>>,
}


impl RecipesState {
    /// loads a recipe file in of the 3 formats.
    /// the base-folder is the RECIPE_FILES_FOLDER (at the top of main.rs)
    pub fn load(&mut self, file_name: &str, format: RecipeFileFormat) -> io::Result<()> {
        println!("Loading {} - {:?} - Before Elements: {}, Recipes: {}", file_name, format, self.num_to_str.len(), self.recipes_ing.len());
        let start_time = Instant::now();

        let file_path = format!("{}/{}", RECIPE_FILES_FOLDER, file_name);
        let file = &mut File::open(file_path)?;

        let response = match format {
            RecipeFileFormat::ICSaveFile => self.load_recipes_gzip(file),
            RecipeFileFormat::JSONRecipesNum => self.load_recipes_num(file),
            RecipeFileFormat::JSONOldDepthExplorerRecipes => self.load_recipes_old_depth_explorer(file),
        };

        match response {
            Err(e) => panic!("  - FAILED TO LOAD... ({:?}): {}", start_time.elapsed(), e),
            Ok(_) => println!("  - Complete! - {:?} - After Elements: {}, Recipes: {}", start_time.elapsed(), self.num_to_str.len(), self.recipes_ing.len()),
        }
        response
    }

    
    /// saves a recipe file in of the 3 formats.
    /// the base-folder is the RECIPE_FILES_FOLDER (at the top of main.rs)
    pub fn save(&self, file_name: &str, format: RecipeFileFormat) -> io::Result<()> {
        println!("Saving {} - {:?} - Elements: {}, Recipes: {}", file_name, format, self.num_to_str.len(), self.recipes_ing.len());
        let start_time = Instant::now();

        let file_path = &format!("{}/{}", RECIPE_FILES_FOLDER, file_name);

        let response = match format {
            RecipeFileFormat::ICSaveFile => self.save_recipes_gzip(file_path),
            RecipeFileFormat::JSONRecipesNum => self.save_recipes_num(file_path),
            RecipeFileFormat::JSONOldDepthExplorerRecipes => self.save_recipes_old_depth_explorer(file_path),
        };

        match response {
            Err(ref e) => println!("  - FAILED TO SAVE... ({:?}): {}", start_time.elapsed(), e),
            Ok(_) => println!("  - Complete! ({:?})", start_time.elapsed()),
        }
        response
    }









    fn load_recipes_num(&mut self, file: &File) -> io::Result<()> {
        let deserialize_time = Instant::now();

        let reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
        let mut data: RecipesNum = serde_json::from_reader(reader)?;
        println!("  - Deserialization complete: {:?}", deserialize_time.elapsed());

        // --- Parallel Processing ---
        let recipe_process_time = Instant::now();
        let mut str_to_num: FxHashMap<String, u32> = data.num_to_str
            .par_iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u32))
            .collect();


        let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::with_capacity_and_hasher(data.num_to_str.len(), Default::default());

        for (first_ingredient, inner_map) in data.recipes.iter() {
            for (second_ingredient, result) in inner_map.iter() {
                recipes_ing.insert(sort_recipe_tuple((*first_ingredient, *second_ingredient)), *result);
            }
        }
        println!("  - Recipe processing complete: {:?}", recipe_process_time.elapsed());

        self.merge_new_variables_with_new(&mut data.num_to_str, &mut str_to_num, recipes_ing);
        Ok(())
    }






    fn save_recipes_num(&self, file_path: &str) -> io::Result<()> {
        let recipe_process_time = Instant::now();
        let mut recipes: FxHashMap<u32, FxHashMap<u32, u32>> = FxHashMap::with_capacity_and_hasher(self.num_to_str.len(), Default::default());

        for (&recipe, &result) in self.recipes_ing.iter() {
            recipes.entry(recipe.0).or_default().insert(recipe.1, result);
        }

        let data = RecipesNum {
            recipes,
            num_to_str: self.num_to_str.clone(),
        };
        println!("  - Recipe Processing complete: {:?}", recipe_process_time.elapsed());


        let file = File::create(file_path)?;
        let mut writer = BufWriter::with_capacity(1024 * 1024, file);
        serde_json::to_writer(&mut writer, &data)?;
        Ok(())
    }


















    fn load_recipes_old_depth_explorer(&mut self, file: &File) -> io::Result<()> {
        let deserialize_time = Instant::now();

        let reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
        let recipes: FxHashMap<String, String> = serde_json::from_reader(reader)?;
        println!("  - Deserialization complete: {:?}", deserialize_time.elapsed());


        let mut num_to_str: Vec<String> = Vec::new();
        let mut str_to_num: FxHashMap<String, u32> = FxHashMap::default();
        let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::with_capacity_and_hasher(recipes.len(), Default::default());

        let mut get_id = |elem: &str| {
            match str_to_num.get(elem) {
                Some(&x) => { x },
                None => {
                    let id = num_to_str.len() as u32;
                    num_to_str.push(elem.to_string());
                    str_to_num.insert(elem.to_string(), id);
                    id
                }
            }
        };

        for (recipe_string, result) in recipes.into_iter() {
            let (first, second) = recipe_string.split_once("=")
                .ok_or(io::Error::new(io::ErrorKind::InvalidData, format!("Invalid Recipe, couldn't split by '=': {}", recipe_string)))?;
            let comb = sort_recipe_tuple((get_id(first), get_id(second)));
            recipes_ing.insert(comb, get_id(&result));
        }

        self.merge_new_variables_with_new(&mut num_to_str, &mut str_to_num, recipes_ing);
        Ok(())
    }







    fn save_recipes_old_depth_explorer(&self, file_path: &str) -> io::Result<()> {
        let recipe_process_time = Instant::now();

        let mut recipes: FxHashMap<String, String> = FxHashMap::with_capacity_and_hasher(self.recipes_ing.len(), Default::default());

        for (&(f, s), &r) in self.recipes_ing.iter() {
            let first = &self.num_to_str[f as usize];
            let second = &self.num_to_str[s as usize];
            let result = self.num_to_str[r as usize].clone();

            let string_recipe = if f < s { (first, second) } else { (second, first) };
            // let comb = format!("{}={}", a.0, a.1);

            let mut comb = String::with_capacity(string_recipe.0.len() + 1 + string_recipe.1.len());
            comb.push_str(string_recipe.0);
            comb.push('=');
            comb.push_str(string_recipe.1);

            recipes.insert(comb, result);
        }
        println!("  - Recipe Processing complete: {:?}", recipe_process_time.elapsed());

        let file = File::create(file_path)?;
        let mut writer = BufWriter::with_capacity(1024 * 1024, file);
        serde_json::to_writer_pretty(&mut writer, &recipes)?;
        Ok(())
    }

















    fn load_recipes_gzip(&mut self, file: &mut File) -> io::Result<()> {
        let deserialize_time = Instant::now();

        // 1. Read compressed data
        let mut gz_buffer = Vec::new();
        file.read_to_end(&mut gz_buffer)?;

        // 2. Get expected size from GZIP footer
        if gz_buffer.len() < 4 { return Err(io::Error::new(io::ErrorKind::InvalidData, "Gzip data too short")); }
        let isize = u32::from_le_bytes(gz_buffer[gz_buffer.len()-4..].try_into().unwrap()) as usize;

        // 3. Decompress
        let mut decompressor = Decompressor::new();
        let mut out_buf = vec![0u8; isize];
        let actual_size = decompressor.gzip_decompress(&gz_buffer, &mut out_buf)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Decompression failed: {:?}", e)))?;
        out_buf.truncate(actual_size); // Adjust size if ISIZE was wrong

        // 4. Parse JSON
        let data: RecipesGzip = serde_json::from_slice(&out_buf)?;
        println!("  - Deserialization complete: {:?}", deserialize_time.elapsed());


        let mut num_to_str: Vec<String> = vec![String::new(); data.items.len()];
        let mut str_to_num: FxHashMap<String, u32> = FxHashMap::default();
        let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::default();

        for item in data.items.into_iter() {
            let id_usize = item.id as usize;
            if id_usize >= num_to_str.len() { num_to_str.resize(id_usize, String::new()); }

            num_to_str[id_usize] = item.text.clone();
            str_to_num.insert(item.text, item.id);

            for recipe in item.recipes.into_iter() {
                recipes_ing.insert(recipe, item.id);
            }
        }

        self.merge_new_variables_with_new(&mut num_to_str, &mut str_to_num, recipes_ing);
        Ok(())
    }










    fn save_recipes_gzip(&self, file_path: &str) -> io::Result<()> {
        let recipes_result_time = Instant::now();
        let mut exact_recipes_result: Vec<Vec<(u32, u32)>> = vec![Vec::new(); self.num_to_str.len()];
        for (&(f, s), &r) in self.recipes_ing.iter() {
            exact_recipes_result[r as usize].push((f, s));
        }
        println!("  - made recipes_result: {:?}", recipes_result_time.elapsed());

        let build_items_vec_time = Instant::now();
        let mut items = Vec::with_capacity(self.num_to_str.len());
        for (id, text) in self.num_to_str.iter().enumerate() {
            items.push(RecipesGzipItemData {
                id: id as u32,
                text: text.clone(),
                recipes: exact_recipes_result[id].clone(),
            });
        }
        drop(exact_recipes_result);
        println!("  - built items vector: {:?}", build_items_vec_time.elapsed());

        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("SystemTime before UNIX EPOCH!")
            .as_millis();

        let gzip_save_data = RecipesGzip {
            name: String::from("pee pee, Poo Poo"),
            version: String::from("1.0"),
            created: now_ms,
            updated: now_ms,
            instances: Vec::new(),
            items,
        };


        let file = File::create(file_path)?;
        let mut writer = BufWriter::with_capacity(1024 * 1024, file);

        // Step 1: Serialize to an in-memory buffer (uses RAM)
        let uncompressed_data = serde_json::to_vec(&gzip_save_data)?;

        // Step 2: Compress the buffer using libdeflate
        let mut compressor = Compressor::new(CompressionLvl::new(1)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid libdeflate compression level"))?);
        
        // Allocate buffer for compressed output. Use `gzip_compress_bound` for safety.
        let mut compressed_buffer = vec![0u8; compressor.gzip_compress_bound(uncompressed_data.len())];

        let actual_compressed_size = compressor.gzip_compress(&uncompressed_data, &mut compressed_buffer)
            .map_err(|e| io::Error::other(format!("libdeflate compression failed: {:?}", e)))?;

        compressed_buffer.resize(actual_compressed_size, 0);
        writer.write_all(&compressed_buffer)?;

        Ok(())
    }











    /// extracts the to_request_recipes and marks them as UNKNOWN_STR
    pub fn extract_to_request(&self) -> RecipesState {
        let mut new_state = RecipesState::new();
        let mut str_to_num = FxHashMap::default();
        
        let start_time = Instant::now();

        for entry in self.to_request_recipes.iter() {
            let (id1, id2) = *entry;

            let new_id1 = new_state.variables_add_element_str(&self.num_to_str[id1 as usize], &mut str_to_num);
            let new_id2 = new_state.variables_add_element_str(&self.num_to_str[id2 as usize], &mut str_to_num);
            
            new_state.recipes_ing.insert(sort_recipe_tuple((new_id1, new_id2)), UNKNOWN_ID);
        }
        
        println!("Extracted {} requests into new state. ({:?})", self.to_request_recipes.len(), start_time.elapsed());
        new_state
    }


    /// Replaces recipes resulting in UNKNOWN_STR with actual results from `other_state`.
    pub fn fill_unknowns_with(&mut self, other_state: &RecipesState) {
        let mut str_to_num = self.get_str_to_num_map();
        let other_map = other_state.get_str_to_num_map();

        let mut updates = Vec::new();
        let start_time = Instant::now();

        for (&(id1, id2), &res) in &self.recipes_ing {
            if res == UNKNOWN_ID || res == NOTHING_ID {
                let name1 = &self.num_to_str[id1 as usize];
                let name2 = &self.num_to_str[id2 as usize];

                // Check if other_state knows about both ingredients and the recipe
                if let (Some(&o_id1), Some(&o_id2)) = (other_map.get(name1), other_map.get(name2)) 
                    && let Some(&o_res) = other_state.recipes_ing.get(&sort_recipe_tuple((o_id1, o_id2))) {
                        let result_name = &other_state.num_to_str[o_res as usize];
                        
                        if o_res != UNKNOWN_ID {
                            updates.push(((id1, id2), result_name.to_string()));
                        }
                    }
            }
        }

        let changed = updates.len();
        // Apply the updates safely outside the iteration
        for (comb, res_name) in updates {
            let new_id = self.variables_add_element_str(&res_name, &mut str_to_num);
            self.recipes_ing.insert(comb, new_id);
        }
        
        println!("Filled in {} unknown recipes! ({:?})", changed, start_time.elapsed());
    }























    fn merge_new_variables_with_new(
        &mut self,
        new_num_to_str: &mut Vec<String>,
        new_str_to_num: &mut FxHashMap<String, u32>,
        new_recipes_ing: FxHashMap<(u32, u32), u32>
    ) {
        println!("  - Merging new Elements: {}, Recipes: {}", new_num_to_str.len(), new_recipes_ing.len());

        let neal_case_time = Instant::now();
        let mut new_neal_case_map: Vec<u32> = Vec::with_capacity(new_num_to_str.len());
        let mut added: Vec<String> = Vec::new();

        for str in new_num_to_str.iter() {
            let neal_str = start_case_unicode(str);
            match new_str_to_num.get(&neal_str) {
                Some(x) => {
                    // neal case version exists, link to it
                    new_neal_case_map.push(*x);
                }
                None => {
                    // neal case version does not exist, create it and link to it
                    let neal_id = (new_num_to_str.len() + added.len()) as u32;
                    new_str_to_num.insert(neal_str.clone(), neal_id);
                    added.push(neal_str.clone());

                    new_neal_case_map.push(neal_id);
                }
            }
        }
        // add all added neal_cased elements into num_to_str
        new_num_to_str.append(&mut added);
        let indices_to_add = new_neal_case_map.len()..new_num_to_str.len();
        new_neal_case_map.extend(indices_to_add.map(|i| i as u32));

        println!("  - nealcase map complete: {:?}", neal_case_time.elapsed());



        // --- Merge with existing Variables ---
        // maps new ids to the old existing ids
        let newnum_to_existingnum_time = Instant::now();

        let mut newnum_to_existingnum: Vec<Option<u32>> = vec![None; new_num_to_str.len()];
        for (existingnum, existingstr) in self.num_to_str.iter().enumerate() {
            if let Some(&newnum) = new_str_to_num.get(existingstr) {
                newnum_to_existingnum[newnum as usize] = Some(existingnum as u32);
            }
        }

        let mut neal_queue = Vec::new();
        // merge new elements over to the existing ones
        for (newnum, newstr) in new_num_to_str.iter().enumerate() {
            if newnum_to_existingnum[newnum].is_none() {
                // newstr is not in existing_num_to_str
                let new_existing_id = self.num_to_str.len();

                self.num_to_str.push(newstr.clone());
                neal_queue.push(newnum);

                newnum_to_existingnum[newnum] = Some(new_existing_id as u32);
            } 
        }

        // finally, merge the neal map
        for newnum in neal_queue.into_iter() {
            self.neal_case_map.push(newnum_to_existingnum[new_neal_case_map[newnum] as usize].unwrap());
        }
        println!("  - newnum to existingnum map complete: {:?}", newnum_to_existingnum_time.elapsed());


        // merge recipes_ing
        let recipes_ing_merge_time = Instant::now();

        let transformed_recipes: Vec<((Element, Element), Element)> = new_recipes_ing
            .par_iter()
            .filter_map(|(&(first, second), &result)| {
                let existing_first = newnum_to_existingnum[first as usize].expect("Missing existing ID for first ingredient");
                let existing_second = newnum_to_existingnum[second as usize].expect("Missing existing ID for second ingredient");
                let existing_result = newnum_to_existingnum[result as usize].expect("Missing existing ID for result");

                let recipe = sort_recipe_tuple((existing_first, existing_second));
                // if new recipe is not NOTHING it always gets added
                // if new recipe is NOTHING it only gets added if the recipe didn't exist at all
                if (existing_result != NOTHING_ID && existing_result != UNKNOWN_ID) || !self.recipes_ing.contains_key(&recipe) {
                    Some((recipe, existing_result))
                }
                else { None }
            })
            .collect();

        self.recipes_ing.extend(transformed_recipes);
        self.num_to_str_len = Self::get_num_to_str_len(&self.num_to_str);

        println!("  - Merging recipes_ing complete: {:?}", recipes_ing_merge_time.elapsed());


        self.verify_recipe_stuff();
    }




    pub fn verify_recipe_stuff(&self) {
        if self.recipes_ing.get(
            &sort_recipe_tuple((self.str_to_num_fn("Fire"), self.str_to_num_fn("Water")))
        ) != Some(&self.str_to_num_fn("Steam")) {
            println!("Water + Fire = Steam is not in recipes_ing")
        }
        assert_eq!(self.str_to_num_fn("Nothing"), 0);  // nothing needs to have id 0
        assert_eq!(self.num_to_str.len(), self.neal_case_map.len());  // if these don't match something went wrong...
    }










    pub fn retain_only_recipes_from_end_of_lineages_file(&mut self, path: String, extra_elements_to_use: &[Element], less_than_depth: usize) {
        // element name -> depth
        // "D:/InfiniteCraft/Codes/rust/Lineages Files/lel.json"
        let json_content = fs::read_to_string(path).unwrap();
        let mut data: FxHashMap<String, usize> = if LINEAGES_FILE_COOL_JSON_MODE {
            let parsed_data: CoolJsonLineagesFile = serde_json::from_str(&json_content).unwrap();
            parsed_data.elements.into_iter().map(|(element_name, lineages)| (element_name, lineages.len())).collect()
        }
        else {
            serde_json::from_str(&json_content).unwrap()
        };
        data.retain(|_, lineages_len| *lineages_len < less_than_depth);

        let str_to_num = self.get_str_to_num_map();
        let mut elements_to_use: FxHashSet<Element> = data.into_keys().map(|x| *str_to_num.get(&start_case_unicode(&x)).unwrap()).collect();
        drop(str_to_num);
        elements_to_use.extend(BASE_IDS.chain(extra_elements_to_use.iter().cloned()));

        println!("elements to use {}", elements_to_use.len());

        println!("before recipe retain: {}", self.recipes_ing.len());
        self.recipes_ing.retain(|(f, s), _| elements_to_use.contains(f) && elements_to_use.contains(s));
        println!("after recipe retain: {}", self.recipes_ing.len());
    }






    pub fn subtract_recipes(&mut self, other_state: &Self) {
        self.recipes_ing.retain(|recipe, _| !other_state.recipes_ing.contains_key(recipe));
    }











    pub fn load_recipes_from_lineages_file(&mut self, file_name: &str) -> io::Result<()> {
        println!("Loading lineages file: {}", file_name);
        let start_time = Instant::now();

        let file_path = format!("{}/{}", RECIPE_FILES_FOLDER, file_name);
        let file = File::open(&file_path)?;
        let reader = BufReader::new(file);

        let mut num_to_str: Vec<String> = Vec::new();
        let mut str_to_num: FxHashMap<String, u32> = FxHashMap::default();
        let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::default();

        // Helper closure to get or create an ID for an element string.
        let mut get_id = |elem: &str| {
            let trimmed_elem = elem.trim();
            if let Some(&id) = str_to_num.get(trimmed_elem) {
                id
            } else {
                let id = num_to_str.len() as u32;
                num_to_str.push(trimmed_elem.to_string());
                str_to_num.insert(trimmed_elem.to_string(), id);
                id
            }
        };

        // Process each line in the file.
        for line_result in reader.lines() {
            let line = line_result?;
            // Split the line into "ing1 + ing2" and "result" parts
            if let Some((ingredients_part, result_part)) = line.split_once(" = ") {
                // Split the ingredients part into "ing1" and "ing2"
                if let Some((first_part, second_part)) = ingredients_part.split_once(" + ") {
                    let first_id = get_id(first_part);
                    let second_id = get_id(second_part);
                    let result_id = get_id(result_part);

                    let recipe = sort_recipe_tuple((first_id, second_id));
                    recipes_ing.insert(recipe, result_id);
                }
            }
        }

        println!("  - Lineage file parsing complete: {:?}", start_time.elapsed());
        
        // Merge the newly loaded data into the main application state.
        self.merge_new_variables_with_new(&mut num_to_str, &mut str_to_num, recipes_ing);

        println!("  - Merging complete! ({:?})", start_time.elapsed());
        
        Ok(())
    }
}