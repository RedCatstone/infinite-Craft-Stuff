use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File}, io::{self, BufWriter, BufReader, Write, Read}, path::{Path, PathBuf}, sync::RwLock, time::{Instant, SystemTime, UNIX_EPOCH}
};
use rustc_hash::FxHashMap;
use rayon::prelude::*;

use libdeflater::{CompressionLvl, Compressor, Decompressor};

use crate::{structures::*, SAVED_RECIPES_FILES_LOCATION};










#[derive(Deserialize, Serialize, Default)]
struct RecipesNum {
    #[serde(default)]
    #[serde(alias = "numToStr")]
    num_to_str: Vec<String>,

    #[serde(default)]
    recipes: FxHashMap<u32, FxHashMap<u32, u32>>,
}

pub fn load_recipes_num(filepath: &str) {
    let start_time = Instant::now();

    let path = Path::new(filepath);
    let file = File::open(path).unwrap_or_else(|_| panic!("path {:?} does not exist", path));
    let reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer

    let data: RecipesNum = serde_json::from_reader(reader).expect("JSON reading failed...");
    println!("  - Deserialization complete: {:?}", start_time.elapsed());


    let mut num_to_str: Vec<String> = if !data.num_to_str.is_empty() { data.num_to_str }
        else { vec![String::from("Nothing"), String::from("Fire"), String::from("Water"), String::from("Earth"), String::from("Wind")] };

    // --- Parallel Processing ---
    let mut str_to_num: FxHashMap<String, u32> = num_to_str
        .par_iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i as u32))
        .collect();


    let recipe_process_time = Instant::now();
    let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::with_capacity_and_hasher(num_to_str.len(), Default::default());



    for (first_ingredient, inner_map) in data.recipes.iter() {
        for (second_ingredient, result) in inner_map.iter() {
            recipes_ing.insert(sort_recipe_tuple((*first_ingredient, *second_ingredient)), *result);
        }
    }
    println!("  - Recipe processing complete: {:?}", recipe_process_time.elapsed());

    merge_new_variables_with_existing(&mut num_to_str, &mut str_to_num, recipes_ing);
    println!("Loaded Recipes from {}: {:?}", filepath, start_time.elapsed());
}






pub fn save_recipes_num(filename: &str) -> io::Result<()> {
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized.");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let num_to_str = variables.num_to_str.read().unwrap();

    let mut data = RecipesNum::default();

    let mut recipes: FxHashMap<u32, FxHashMap<u32, u32>> = FxHashMap::with_capacity_and_hasher(recipes_ing.len(), Default::default());

    for (&(first, second), &result) in recipes_ing.iter() {
        let comb = if first < second { (first, second) } else { (second, first) };

        recipes.entry(comb.0).or_default().insert(comb.1, result);
    }
    
    data.recipes = recipes;
    data.num_to_str = num_to_str.clone();


    let folder_path = PathBuf::from(SAVED_RECIPES_FILES_LOCATION);
    fs::create_dir_all(&folder_path)?;
    let full_path = folder_path.join(filename);

    let file = File::create(&full_path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);

    serde_json::to_writer(&mut writer, &data).expect("JSON seriliaziation failed...");

    println!("Saved recipesNum to {:?}: {:?}", full_path, start_time.elapsed());
    Ok(())
}


















pub fn load_recipes_old_depth_explorer(filepath: &str) {
    let start_time = Instant::now();

    let path = Path::new(filepath);
    let file = File::open(path).unwrap_or_else(|_| panic!("path {:?} does not exist", path));
    let reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer

    let recipes: FxHashMap<String, String> = serde_json::from_reader(reader).expect("JSON reading failed...");
    println!("  - Deserialization complete: {:?}", start_time.elapsed());


    let mut num_to_str: Vec<String> = vec![String::from("Nothing")];
    let mut str_to_num: FxHashMap<String, u32> = num_to_str.iter().enumerate().map(|(i, str)| (str.clone(), i as u32)).collect();
    let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::default();


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
        let (first, second) = recipe_string.split_once("=").expect("recipe_str did not have an '='");

        let comb = sort_recipe_tuple((get_id(first), get_id(second)));
        recipes_ing.insert(comb, get_id(&result));
    }

    merge_new_variables_with_existing(&mut num_to_str, &mut str_to_num, recipes_ing);
    println!("Loaded Recipes from {}: {:?}", filepath, start_time.elapsed());
}







pub fn save_recipes_old_depth_explorer(filename: &str) -> io::Result<()> {
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized.");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let num_to_str = variables.num_to_str.read().unwrap();

    let mut recipes: FxHashMap<String, String> = FxHashMap::with_capacity_and_hasher(recipes_ing.len(), Default::default());

    for (&(f, s), &r) in recipes_ing.iter() {
        let first = &num_to_str[f as usize];
        let second = &num_to_str[s as usize];
        let result = &num_to_str[r as usize];

        let a = if f < s { (first, second) } else { (second, first) };
        // let comb = format!("{}={}", a.0, a.1);

        let mut comb = String::with_capacity(a.0.len() + 1 + a.1.len());
        comb.push_str(a.0);
        comb.push('=');
        comb.push_str(a.1);

        recipes.insert(comb, result.to_string());
    }


    let folder_path = PathBuf::from(SAVED_RECIPES_FILES_LOCATION);
    fs::create_dir_all(&folder_path)?;
    let full_path = folder_path.join(filename);

    let file = File::create(&full_path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);

    serde_json::to_writer_pretty(&mut writer, &recipes).expect("JSON seriliaziation failed...");

    println!("Saved old depth_explorer_recipes to {:?}: {:?}", full_path, start_time.elapsed());
    Ok(())
}


















#[derive(Deserialize, Serialize)]
struct RecipesGzip {
    name: String,
    version: String,
    created: u128, // Milliseconds since UNIX epoch
    updated: u128, // Milliseconds since UNIX epoch
    instances: Vec<()>,
    items: Vec<RecipesGzipItemData>,
}
#[derive(Deserialize, Serialize)]
struct RecipesGzipItemData {
    #[serde(default)]
    id: u32,
    text: String,

    #[serde(default)]
    recipes: Vec<(u32, u32)>,
}

pub fn load_recipes_gzip(filepath: &str) -> io::Result<()> {
    let start_time = Instant::now();

    // 1. Read compressed data
    let mut file = File::open(filepath)?;
    let mut gz_data = Vec::new();
    file.read_to_end(&mut gz_data)?;

    // 2. Get expected size from GZIP footer
    if gz_data.len() < 4 { return Err(io::Error::new(io::ErrorKind::InvalidData, "Gzip data too short")); }
    let isize = u32::from_le_bytes(gz_data[gz_data.len()-4..].try_into().unwrap()) as usize;

    // 3. Decompress
    let mut decompressor = Decompressor::new();
    let mut out_buf = vec![0u8; isize];
    let actual_size = decompressor.gzip_decompress(&gz_data, &mut out_buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("Decompression failed: {:?}", e)))?;
    out_buf.truncate(actual_size); // Adjust size if ISIZE was wrong

    // 4. Parse JSON
    let data: RecipesGzip = serde_json::from_slice(&out_buf)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("JSON parsing failed: {}", e)))?;
    println!("  - Deserialization complete: {:?}", start_time.elapsed());





    let mut num_to_str: Vec<String> = vec![String::from("Nothing")];
    let mut str_to_num: FxHashMap<String, u32> = num_to_str.iter().enumerate().map(|(i, str)| (str.clone(), i as u32)).collect();
    let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::default();

    let mut old_id_to_new_id: FxHashMap<u32, u32> = FxHashMap::default();

    for item in data.items.iter() {
        let id = if item.text == *"Nothing" { 0 } else { num_to_str.len() as u32 };
        num_to_str.push(item.text.to_string());
        str_to_num.insert(item.text.to_string(), id);
        old_id_to_new_id.insert(item.id, id);
    }
    
    for item in data.items.iter() {
        let r = *old_id_to_new_id.get(&item.id).expect("Id does not exist?!");

        for (first, second) in item.recipes.iter() {
            let f = *old_id_to_new_id.get(first).expect("Id does not exist?!");
            let s = *old_id_to_new_id.get(second).expect("Id does not exist?!");
            let comb = if f < s { (f, s) } else { (s, f) };

            recipes_ing.insert(comb, r);
        }
    }

    merge_new_variables_with_existing(&mut num_to_str, &mut str_to_num, recipes_ing);
    println!("Loaded Recipes from {}: {:?}", filepath, start_time.elapsed());

    Ok(())
}










pub fn save_recipes_gzip(filename: &str, save_name: &str) -> io::Result<()> {
    let start_time = Instant::now();

    let variables = VARIABLES.get().expect("VARIABLES not initialized.");

    // --- Acquire Read Locks ---
    let num_to_str = variables.num_to_str.read().unwrap();
    let recipes_ing = variables.recipes_ing.read().unwrap();

    let mut recipes_result: FxHashMap<u32, Vec<(u32, u32)>> = FxHashMap::with_capacity_and_hasher(num_to_str.len(), Default::default());
    for (&recipe, &r) in recipes_ing.iter().take(16777215) {
        recipes_result.entry(r).or_default().push(recipe);
    }
    println!("  - updated recipes_uses in {:?}", start_time.elapsed());



    let mut items: Vec<RecipesGzipItemData> = Vec::with_capacity(num_to_str.len());

    let build_items_vec_time = Instant::now();
    for (id_u32, text) in num_to_str.iter().enumerate().map(|(i, s)| (i as u32, s)) {
        items.push(RecipesGzipItemData {
            id: id_u32,
            text: text.clone(),         // Clone the string for owned ItemData
            recipes: recipes_result.get(&id_u32).cloned().unwrap_or_default(), // Clone vec or provide empty,
        });
    }
    println!("  - built items vector: {:?}", build_items_vec_time.elapsed());

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX EPOCH!")
        .as_millis();

    let gzip_save_data = RecipesGzip {
        name: save_name.to_string(),
        version: String::from("1.0"),
        created: now_ms,
        updated: now_ms,
        instances: Vec::new(),
        items,
    };


    let folder_path = PathBuf::from(SAVED_RECIPES_FILES_LOCATION);
    fs::create_dir_all(&folder_path)?;
    let full_path = folder_path.join(filename);

    let file = File::create(&full_path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);

    // Step 1: Serialize to an in-memory buffer (uses RAM)
    let uncompressed_data = serde_json::to_vec(&gzip_save_data)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("JSON serialization to vec failed: {}", e)))?;

    // Step 2: Compress the buffer using libdeflate
    let mut compressor = Compressor::new(CompressionLvl::new(1)
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "Invalid libdeflate compression level"))?);
    
    // Allocate buffer for compressed output. Use `gzip_compress_bound` for safety.
    let mut compressed_buffer = vec![0u8; compressor.gzip_compress_bound(uncompressed_data.len())];

    let actual_compressed_size = compressor.gzip_compress(&uncompressed_data, &mut compressed_buffer)
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("libdeflate compression failed: {:?}", e)))?;

    compressed_buffer.resize(actual_compressed_size, 0);
    writer.write_all(&compressed_buffer)?;


    println!("Saved savefile to {:?}: {:?}", full_path, start_time.elapsed());
    Ok(())
}























fn merge_new_variables_with_existing(new_num_to_str: &mut Vec<String>, new_str_to_num: &mut FxHashMap<String, u32>, new_recipes_ing: FxHashMap<(u32, u32), u32>) {

    let base_elements: [u32; 4] = ["Fire", "Water", "Earth", "Wind"].map(|x| {
        *new_str_to_num
            .get(x)
            .unwrap_or_else(|| panic!("Base element '{}' not found in str_to_num map", x))
    });


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




    // --- Try to get existing Variables or initialize ---
    let merge_time = Instant::now();

    match VARIABLES.get() {
        None => {
            // --- First time loading: Initialize ---
            let new_variables = Variables {
                base_elements,
                num_to_str: RwLock::new(new_num_to_str.to_owned()),
                neal_case_map: RwLock::new(new_neal_case_map),
                recipes_ing: RwLock::new(new_recipes_ing),
                ..Default::default()
            };
            VARIABLES.set(new_variables).expect("Failed to set global VARIABLES");
        }
        Some(old_variables) => {

            // --- Variables already exist: Merge ---            
            let mut existing_recipes_ing = old_variables.recipes_ing.write().unwrap();
            let mut existing_neal_case_map = old_variables.neal_case_map.write().unwrap();
            let mut existing_num_to_str = old_variables.num_to_str.write().unwrap();


            // maps new ids to the old existing ids
            let mut newnum_to_existingnum: Vec<Option<u32>> = vec![None; new_num_to_str.len()];
            for (existingnum, existingstr) in existing_num_to_str.iter().enumerate() {
                if let Some(newnum) = new_str_to_num.get(existingstr) {
                    newnum_to_existingnum[*newnum as usize] = Some(existingnum as u32);
                }
            }

            let mut neal_queue = Vec::new();
            // merge new elements over to the existing ones
            for (newnum, newstr) in new_num_to_str.iter().enumerate() {
                if newnum_to_existingnum[newnum].is_none() {
                    // newstr is not in existing_num_to_str
                    let new_existing_id = existing_num_to_str.len();

                    existing_num_to_str.push(newstr.clone());
                    neal_queue.push(newnum);

                    newnum_to_existingnum[newnum] = Some(new_existing_id as u32);
                } 
            }

            // finally, merge the neal map
            for newnum in neal_queue.into_iter() {
                existing_neal_case_map.push(newnum_to_existingnum[new_neal_case_map[newnum] as usize].unwrap());
            }


            for (&(first, second), &result) in new_recipes_ing.iter() {
                let comb = sort_recipe_tuple((newnum_to_existingnum[first as usize].unwrap(), newnum_to_existingnum[second as usize].unwrap()));
                existing_recipes_ing.insert(
                    comb,
                    newnum_to_existingnum[result as usize].unwrap()
                );
            }


            println!("  - Merging complete: {:?}", merge_time.elapsed());
        }
    }
}