#![allow(dead_code)]
mod structures;
mod recipe_loader;
mod lineage;
mod depth_explorer;
mod recipe_requestor;

use std::time::Duration;

use crate::structures::*; // Import public static if needed directly
use crate::lineage::*;
use crate::depth_explorer::*;






const RECIPE_FILES_FOLDER: &'static str = "Recipe Files";
const DEPTH_EXPLORER_MAX_SEED_LENGTH: usize = 4;


const DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS: usize = 15;





#[tokio::main]
async fn main() {

    // --- LOAD RECIPES ---
    // there are 3 formats. if you load multiple recipe files, it merges them
    // recipe_loader::load("depth_explorer_recipes.json", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();

    // -- Analyzer / Savefile Format --
    // recipe_loader::save("full_db.ic", recipe_loader::RecipeFileFormat::ICSaveFile).unwrap();

    // -- Auto Save --
    // when this _auto_save goes out of scope, it saves 1 final time
    // let _auto_save = auto_load_and_save_recipes(
    //     Duration::from_secs(30 * 60),
    //     "depth_explorer_recipes.json",
    //     recipe_loader::RecipeFileFormat::JSONRecipesNum
    // );


    test_depth_explorer().await;

}










async fn test_depth_explorer() {
    recipe_loader::load("depth_explorer_recipes - Punc 8.json", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_SEED_LENGTH,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 0,
        lineage_elements: string_lineage_results(r#"
Earth + Water = Plant
Earth + Plant = Tree
Tree + Water = River
Earth + River = Delta
River + Tree = Paper
Paper + Tree = Book
Book + Delta = Alphabet
Alphabet + Alphabet = Word
Word + Word = Sentence
Sentence + Wind = Phrase
Book + Phrase = Quote
Alphabet + Quote = Punctuation

            "#),
        ..Default::default()
    };

    let encountered = depth_explorer_split_start(&de_vars).await;
    generate_lineages_file(&de_vars, encountered).expect("could not generate lineages file...");
}










async fn test_lineage_stuff () {
    // --- LINEAGE GENERATION STUFF ---

    recipe_loader::load("depth_explorer_recipes", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();

    let recipes_result_map = get_recipes_result_map();
    let recipes_uses_map = get_recipes_uses_map();
    let mut heuristic_map = get_element_heuristic_map(&recipes_uses_map);


    let punc_alts = generate_lineage_multiple_methods(&["Punctuation", "Alphabet", "Delta"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, false);
    punc_alts.print_lineages_ordered();

    let ass_lineage = string_lineage_to_lineage(r#"
 Earth + Wind = Dust
 Dust + Earth = Planet
 Fire + Planet = Sun
 Sun + Water = Rainbow
 Earth + Earth = Mountain
 Mountain + Rainbow = Unicorn
 Earth + Water = Plant
 Fire + Water = Steam
 Fire + Steam = Engine
 Engine + Plant = Car
 Car + Earth = Tire
 Tire + Unicorn = Puncture
 Sun + Wind = Sunflower
 Fire + Wind = Smoke
 Smoke + Sunflower = Smoke Signal
 Smoke Signal + Puncture = Punctuation
 "#);

    let improved_lineage = improve_lineage_depth_explorer(ass_lineage, 1, 0).await;
    improved_lineage.print_lineages_ordered();
}









async fn do_punc_8() {
    // when this _auto_save goes out of scope, it saves 1 final time
    let _auto_save = auto_load_and_save_recipes(
        Duration::from_secs(30 * 60),
        "depth_explorer_recipes - Punc 8.json",
        recipe_loader::RecipeFileFormat::JSONRecipesNum
    );

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_SEED_LENGTH,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 2,
        lineage_elements: string_lineage_results(r#"

Earth + Water = Plant
Earth + Plant = Tree
Tree + Water = River
Earth + River = Delta
River + Tree = Paper
Paper + Tree = Book
Book + Delta = Alphabet
Alphabet + Alphabet = Word
Word + Word = Sentence
Sentence + Wind = Phrase
Book + Phrase = Quote
Alphabet + Quote = Punctuation

            "#),
        ..Default::default()
    };

    let encountered = depth_explorer_split_start(&de_vars).await;
    generate_lineages_file(&de_vars, encountered).unwrap();
}