mod structures;
mod load_recipes;
mod lineage;
mod depth_explorer;
mod recipe_requestor;

use std::time::Duration;

use load_recipes::*;
use crate::structures::*; // Import public static if needed directly
use crate::lineage::*;
use crate::depth_explorer::*;






const SAVED_RECIPES_FILES_LOCATION: &'static str = "Recipe Files Out";
const DEPTH_EXPLORER_MAX_SEED_LENGTH: usize = 8;


const DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS: usize = 15;
const DEPTH_EXPLORER_PRINT_PROGRESS_EVERY_ELEMENTS: usize = 1000;





#[tokio::main]
async fn main() {

    // --- LOAD RECIPES ---
    // there are 3 load_recipes_xxx functions. if multiple recipe files are loaded, it merges them

    // load_recipes_num("D:\\InfiniteCraft\\Codes\\recipesNum.json");
    // load_recipes_old_depth_explorer("D:\\InfiniteCraft\\Codes\\recipes.json");
    // load_recipes_gzip("./Recipe Files Out/full_db.ic").expect("a");


    // loading from auto save:
    load_recipes_num("D:/InfiniteCraft/Codes/rust/Recipe Files Out/depth_explorer_recipes - Punc 8.json");
    // auto save:
    let auto_save = auto_save_recipes(Duration::from_secs(120), || {
        println!("saving recipes...");
        save_recipes_num("depth_explorer_recipes - Punc 8.json").expect("could not auto save...")
    });

    // v analyzer format!!! v
    // save_recipes_gzip("full_db.ic", "Full Db").expect("could not save...");


    // verify recipes:
    {
        let variables = VARIABLES.get().expect("VARIABLES not initialized...");
        let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized...");
        let num_to_str = variables.num_to_str.read().expect("num_to_str not initialized");
        let neal_case_map = variables.neal_case_map.read().expect("neal_case_map not initialized");
        assert_eq!(*recipes_ing.get(&sort_recipe_tuple((str_to_num_fn("Fire"), str_to_num_fn("Water")))).expect("'Water + Fire' is not in recipes_ing"), str_to_num_fn("Steam"));
        assert_eq!(str_to_num_fn("Nothing"), 0);  // nothing needs to have id 0
        assert_eq!(num_to_str.len(), neal_case_map.len());  // if these don't match something went wrong...
    }



    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_SEED_LENGTH,  // modify the global variable so that the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 2,
        lineage_elements: string_lineage_results(r#"

Water + Earth = Plant
Plant + Earth = Tree
Water + Tree = River
River + Earth = Delta
Tree + River = Paper
Tree + Paper = Book
Delta + Book = Alphabet
Alphabet + Alphabet = Word
Word + Word = Sentence
Wind + Sentence = Phrase
Phrase + Book = Quote
Quote + Alphabet = Punctuation

"#),
        ..Default::default()
    };


    let encountered = depth_explorer_split_start(&de_vars).await;

    auto_save.save_now();

    generate_lineages_file(&de_vars, encountered).expect("could not generate lineages file...");








    // lineage stuff
    
    // init_heuristic();
    // let binding = [str_to_num_fn("Cat")];
    // let lineage = generate_lineage(&binding, 1);
    // println!("{}", format_lineage(lineage));
    // let lineage = remove_unneccessary(generate_lineage(&binding, 1));
    // println!("{}", format_lineage(lineage));
}