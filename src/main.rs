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

struct GlobalOptions {
    saved_recipes_files_location: &'static str,
}








const GLOBAL_OPTIONS: GlobalOptions = GlobalOptions {
    saved_recipes_files_location: "Recipe Files Out",
};



const DEPTH_EXPLORER_OPTIONS: DepthExplorerOptions = DepthExplorerOptions {
    stop_after_depth: 5,
    final_elements_guess: 55000,
    final_seeds_guess: 15_000_000,
    input_text_lineage: r#"

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
"#,
};




#[tokio::main]
async fn main() {

    // --- LOAD RECIPES ---
    // you can load using the 3 load_recipes_xxx functions. if you load multiple files, it merges them

    load_recipes_num("D:\\InfiniteCraft\\Codes\\recipesNum.json");
    // load_recipes_gzip("./Recipe Files In/Helper-Save.ic");


    // auto save / auto load:
    load_recipes_num("./Recipe Files Out/depth_explorer_recipes.json");
    auto_save_recipes(Duration::from_secs(60), || save_recipes_num("depth_explorer_recipes.json"));


    // verify recipes:
    {
        let variables = VARIABLES.get().expect("VARIABLES not initialized...");
        let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized...");
        assert_eq!(*recipes_ing.get(&sort_recipe_tuple((str_to_num_fn("Fire"), str_to_num_fn("Water")))).expect("'Water + Fire' is not in recipes_ing"), str_to_num_fn("Steam"));
        assert_eq!(str_to_num_fn("Nothing"), 0);  // nothing needs to have id 0
    }



    depth_explorer_start().await;

    save_recipes_num("depth_explorer_recipes.json").expect("could not save...");

    generate_lineages_file().expect("could not generate lineages file...");








    // lineage stuff
    
    // init_heuristic();
    // let binding = [str_to_num_fn("Cat")];
    // let lineage = generate_lineage(&binding, 1);
    // println!("{}", format_lineage(lineage));
    // let lineage = remove_unneccessary(generate_lineage(&binding, 1));
    // println!("{}", format_lineage(lineage));
}