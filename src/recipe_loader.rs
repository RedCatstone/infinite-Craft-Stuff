use serde::{Deserialize, Serialize};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use std::sync::RwLock;
use std::io::{self, BufWriter, BufReader, Write, Read};
use std::fs::File;
use rustc_hash::FxHashMap;
use rayon::prelude::*;

use libdeflater::{CompressionLvl, Compressor, Decompressor};

use crate::{structures::*, RECIPE_FILES_FOLDER};




#[derive(Debug, Copy, Clone)]
pub enum RecipeFileFormat {
    ICSaveFile,
    JSONRecipesNum,
    JSONOldDepthExplorerRecipes
}


pub fn load(file_name: &str, format: RecipeFileFormat) -> io::Result<()> {
    println!("Loading {} ({:?})", file_name, format);
    let start_time = Instant::now();

    let file_path = format!("{}/{}", RECIPE_FILES_FOLDER, file_name);
    let file = &mut File::open(file_path)?;

    let response = match format {
        RecipeFileFormat::ICSaveFile => load_recipes_gzip(file),
        RecipeFileFormat::JSONRecipesNum => load_recipes_num(file),
        RecipeFileFormat::JSONOldDepthExplorerRecipes => load_recipes_old_depth_explorer(file),
    };

    match response {
        Err(e) => panic!(" - FAILED TO LOAD... ({:?}): {}", start_time.elapsed(), e),
        Ok(_) => println!(" - Complete! ({:?})", start_time.elapsed()),
    }
    response
}


pub fn save(file_name: &str, format: RecipeFileFormat) -> io::Result<()> {
    println!("Saving {} ({:?})", file_name, format);
    let start_time = Instant::now();

    let file_path = &format!("{}/{}", RECIPE_FILES_FOLDER, file_name);

    let response = match format {
        RecipeFileFormat::ICSaveFile => save_recipes_gzip(file_path),
        RecipeFileFormat::JSONRecipesNum => save_recipes_num(file_path),
        RecipeFileFormat::JSONOldDepthExplorerRecipes => save_recipes_old_depth_explorer(file_path),
    };

    match response {
        Err(ref e) => println!(" - FAILED TO SAVE... ({:?}): {}", start_time.elapsed(), e),
        Ok(_) => println!(" - Complete! ({:?})", start_time.elapsed()),
    }
    response
}










#[derive(Deserialize, Serialize, Default)]
struct RecipesNum {
    #[serde(default)]
    #[serde(alias = "numToStr")]
    num_to_str: Vec<String>,

    #[serde(default)]
    recipes: FxHashMap<u32, FxHashMap<u32, u32>>,
}

fn load_recipes_num(file: &File) -> io::Result<()> {
    let deserialize_time = Instant::now();

    let reader = BufReader::with_capacity(1024 * 1024, file); // 1MB buffer
    let mut data: RecipesNum = serde_json::from_reader(reader)?;
    println!("  - Deserialization complete: {:?}", deserialize_time.elapsed());

    // --- Parallel Processing ---
    let mut str_to_num: FxHashMap<String, u32> = data.num_to_str
        .par_iter()
        .enumerate()
        .map(|(i, s)| (s.clone(), i as u32))
        .collect();


    let recipe_process_time = Instant::now();
    let mut recipes_ing: FxHashMap<(u32, u32), u32> = FxHashMap::with_capacity_and_hasher(data.num_to_str.len(), Default::default());

    for (first_ingredient, inner_map) in data.recipes.iter() {
        for (second_ingredient, result) in inner_map.iter() {
            recipes_ing.insert(sort_recipe_tuple((*first_ingredient, *second_ingredient)), *result);
        }
    }
    println!("  - Recipe processing complete: {:?}", recipe_process_time.elapsed());

    merge_new_variables_with_existing(&mut data.num_to_str, &mut str_to_num, recipes_ing);
    Ok(())
}






fn save_recipes_num(file_path: &str) -> io::Result<()> {
    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized.");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let num_to_str = variables.num_to_str.read().unwrap();

    let recipe_process_time = Instant::now();
    let mut recipes: FxHashMap<u32, FxHashMap<u32, u32>> = FxHashMap::with_capacity_and_hasher(num_to_str.len(), Default::default());

    for (&recipe, &result) in recipes_ing.iter() {
        recipes.entry(recipe.0).or_default().insert(recipe.1, result);
    }

    let data = RecipesNum {
        recipes,
        num_to_str: num_to_str.clone(),
    };
    println!(" - Recipe Processing complete: {:?}", recipe_process_time.elapsed());


    let file = File::create(file_path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    serde_json::to_writer(&mut writer, &data)?;
    Ok(())
}


















fn load_recipes_old_depth_explorer(file: &File) -> io::Result<()> {
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

    merge_new_variables_with_existing(&mut num_to_str, &mut str_to_num, recipes_ing);
    Ok(())
}







fn save_recipes_old_depth_explorer(file_path: &str) -> io::Result<()> {
    let recipe_process_time = Instant::now();

    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized.");
    let recipes_ing = variables.recipes_ing.read().unwrap();
    let num_to_str = variables.num_to_str.read().unwrap();

    let mut recipes: FxHashMap<String, String> = FxHashMap::with_capacity_and_hasher(recipes_ing.len(), Default::default());

    for (&(f, s), &r) in recipes_ing.iter() {
        let first = &num_to_str[f as usize];
        let second = &num_to_str[s as usize];
        let result = num_to_str[r as usize].clone();

        let string_recipe = if f < s { (first, second) } else { (second, first) };
        // let comb = format!("{}={}", a.0, a.1);

        let mut comb = String::with_capacity(string_recipe.0.len() + 1 + string_recipe.1.len());
        comb.push_str(string_recipe.0);
        comb.push('=');
        comb.push_str(string_recipe.1);

        recipes.insert(comb, result);
    }
    println!(" - Recipe Processing complete: {:?}", recipe_process_time.elapsed());

    let file = File::create(file_path)?;
    let mut writer = BufWriter::with_capacity(1024 * 1024, file);
    serde_json::to_writer_pretty(&mut writer, &recipes)?;
    Ok(())
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

fn load_recipes_gzip(file: &mut File) -> io::Result<()> {
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

    merge_new_variables_with_existing(&mut num_to_str, &mut str_to_num, recipes_ing);
    Ok(())
}










fn save_recipes_gzip(file_path: &str) -> io::Result<()> {
    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized.");
    let num_to_str = variables.num_to_str.read().unwrap();

    
    let recipes_result_time = Instant::now();
    let recipes_result = get_recipes_result_map();
    println!("  - made recipes_result: {:?}", recipes_result_time.elapsed());

    let build_items_vec_time = Instant::now();
    let mut items: Vec<RecipesGzipItemData> = Vec::with_capacity(num_to_str.len());
    for (id, text) in num_to_str.iter().enumerate() {
        items.push(RecipesGzipItemData {
            id: id as u32,
            text: text.clone(),
            recipes: recipes_result[id].clone(),
        });
    }
    drop(recipes_result);
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
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("libdeflate compression failed: {:?}", e)))?;

    compressed_buffer.resize(actual_compressed_size, 0);
    writer.write_all(&compressed_buffer)?;

    Ok(())
}























fn merge_new_variables_with_existing(new_num_to_str: &mut Vec<String>, new_str_to_num: &mut FxHashMap<String, u32>, new_recipes_ing: FxHashMap<(u32, u32), u32>) {
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
    let existing_variables = match GLOBAL_VARS.get() {
        Some(x) => x,
        None => {
            // first time setting, use the hardcoded default id stuff
            GLOBAL_VARS.set(GlobalVars {
                // vec!["Nothing", ...BASE_ELEMENTS]
                num_to_str: RwLock::new(std::iter::once("Nothing").chain(BASE_ELEMENTS.iter().copied()).map(|x| x.to_string()).collect()),
                // vec![0, 1, 2, 3, ...] mapping to itself
                neal_case_map: RwLock::new((0..=BASE_ELEMENTS.len() as Element).collect()),
                ..Default::default()
            }).expect("could nto set VARIABLES...");

            GLOBAL_VARS.get().unwrap()
        }
    };

    // --- Merge with existing Variables ---
    {
        let mut existing_recipes_ing = existing_variables.recipes_ing.write().unwrap();
        let mut existing_neal_case_map = existing_variables.neal_case_map.write().unwrap();
        let mut existing_num_to_str = existing_variables.num_to_str.write().unwrap();


        // maps new ids to the old existing ids
        let newnum_to_existingnum_time = Instant::now();

        let mut newnum_to_existingnum: Vec<Option<u32>> = vec![None; new_num_to_str.len()];
        for (existingnum, existingstr) in existing_num_to_str.iter().enumerate() {
            if let Some(&newnum) = new_str_to_num.get(existingstr) {
                newnum_to_existingnum[newnum as usize] = Some(existingnum as u32);
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
        println!("  - newnum to existingnum map complete: {:?}", newnum_to_existingnum_time.elapsed());


        // merge recipes_ing
        let recipes_ing_merge_time = Instant::now();

        let transformed_recipes: Vec<((Element, Element), Element)> = new_recipes_ing
            .par_iter()
            .map(|(&(first, second), &result)| {
                let existing_first = newnum_to_existingnum[first as usize].expect("Missing existing ID for first ingredient");
                let existing_second = newnum_to_existingnum[second as usize].expect("Missing existing ID for second ingredient");
                let existing_result = newnum_to_existingnum[result as usize].expect("Missing existing ID for result");

                (sort_recipe_tuple((existing_first, existing_second)), existing_result)
            })
            .collect();

        existing_recipes_ing.extend(transformed_recipes);

        println!("  - Merging recipes_ing complete: {:?}", recipes_ing_merge_time.elapsed());
    }

    verify_recipe_stuff();
}




pub fn verify_recipe_stuff() {
    let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized...");
    let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized...");
    let num_to_str = variables.num_to_str.read().expect("num_to_str not initialized");
    let neal_case_map = variables.neal_case_map.read().expect("neal_case_map not initialized");
    assert_eq!(*recipes_ing.get(&sort_recipe_tuple((str_to_num_fn("Fire"), str_to_num_fn("Water")))).expect("'Water + Fire' is not in recipes_ing"), str_to_num_fn("Steam"));
    assert_eq!(str_to_num_fn("Nothing"), 0);  // nothing needs to have id 0
    assert_eq!(num_to_str.len(), neal_case_map.len());  // if these don't match something went wrong...
}