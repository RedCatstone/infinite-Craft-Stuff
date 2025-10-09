#![allow(dead_code)]
mod structures;
mod recipe_loader;
mod lineage;
mod depth_explorer;
mod recipe_requestor;

use std::time::Duration;

use crate::structures::*;
use crate::lineage::*;
use crate::depth_explorer::*;





const LINEAGES_FILE_COOL_JSON_MODE: bool = true;
const RECIPE_FILES_FOLDER: &'static str = "Recipe Files";
const DEPTH_EXPLORER_MAX_SEED_LENGTH: usize = 9;


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

    let _auto_save = auto_load_and_save_recipes(
        Duration::from_secs(5 * 60),
        "from_base.json",
        recipe_loader::RecipeFileFormat::JSONRecipesNum
    );

    test_depth_explorer().await;


}










async fn test_depth_explorer() {

    // rerequest_all_nothing_recipes().await;

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_SEED_LENGTH,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 0,
        lineage_elements: string_lineage_results(r#"




            "#),
        ..Default::default()
    };

    let encountered = depth_explorer_split_start(&de_vars).await;
    generate_lineages_file(&de_vars, encountered).expect("could not generate lineages file...");
}




async fn test_caps() {
    {
        let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized...");
        let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized...");

        let result_num = recipes_ing.get(&sort_recipe_tuple((str_to_num_fn("Rocket"), str_to_num_fn("Cloud")))).expect("'Cloud + Rocket' is not in recipes_ing");

        println!("result: {}", num_to_str_fn(*result_num));
        
        let to_request = sort_recipe_tuple((str_to_num_fn("Rocket"), str_to_num_fn("Cloud")));
        variables.to_request_recipes.insert(to_request);
    }
    

    println!("{:?}", recipe_requestor::process_all_to_request_recipes().await);

    {
        let variables = GLOBAL_VARS.get().expect("VARIABLES not initialized...");
        let recipes_ing = variables.recipes_ing.read().expect("recipes_ing not initialized...");

        let result_num = recipes_ing.get(&sort_recipe_tuple((str_to_num_fn("Rocket"), str_to_num_fn("Cloud")))).expect("'Cloud + Rocket' is not in recipes_ing");

        println!("result: {}", num_to_str_fn(*result_num));
        
        let to_request = sort_recipe_tuple((str_to_num_fn("Rocket"), str_to_num_fn("Cloud")));
        variables.to_request_recipes.insert(to_request);
    }
}










async fn test_lineage_stuff () {
    // --- LINEAGE GENERATION STUFF ---

    // recipe_loader::load("depth_explorer_recipes.json", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();
    let _auto_save = auto_load_and_save_recipes(
        Duration::from_secs(30 * 60),
        "depth_explorer_recipes.json",
        recipe_loader::RecipeFileFormat::JSONRecipesNum
    );

    let recipes_result_map = get_recipes_result_map();
    let recipes_uses_map = get_recipes_uses_map();
    let mut heuristic_map = get_element_heuristic_map(&recipes_uses_map);

    generate_lineage_multiple_methods(&["Unova Cap Pikachu"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, true);


    let punc_alts = generate_lineage_multiple_methods(&["Punctuation", "Alphabet", "Delta"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, false);
    punc_alts.print_lineages_ordered();

    let ass_lineage = string_lineage_to_lineage(r#"
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
Earth + Punctuation = Period
Period + Wind = Comma
Comma + Period = Semicolon
Quote + Semicolon = Quotation Mark
Alphabet + Fire = Alphabet Soup
Alphabet Soup + Quotation Mark = "Alphabet Soup"
Tree + Tree = Forest
"Alphabet Soup" + Forest = "alphabet Trees"
"alphabet Trees" + Wind = "alphabet Leaves"
"alphabet Leaves" + Period = "alphabet Period"
"alphabet Period" + Quote = "Quotation Marks"
Quote + Quote = Wisdom
Period + Period = Full Stop
Book + Full Stop = End
Alphabet Soup + Tree = Apple
Apple + Word = iPad
Alphabet + iPad = App
App + App = App Store
App Store + End = Append
Append + Wisdom = Prepend
"Quotation Marks" + Prepend = "Prepend"
"Quotation Marks" + Punctuation = "Punctuation"
"Punctuation" + Semicolon = "semicolon"
"semicolon" + Comma = ";,"
";," + Word = " ";
" "; + "Prepend" = "prepend ";
Alphabet + Earth = Planet
Planet + Planet = Star
Alphabet + Star = Asterisk
"prepend "; + Asterisk = "prepend *"
App + Quote = Tweet
Phrase + Tweet = Hashtag
"prepend *" + Hashtag = Prepend Hashtag
" "; + Quotation Mark = " "
" " + "prepend "; = "prepend "
Append + Punctuation = Parenthesis
Hashtag + Word = Trend
Phrase + Trend = Meme
Meme + Parenthesis = ( ͡° ͜ʖ ͡°)
Semicolon + Semicolon = Colon
( ͡° ͜ʖ ͡°) + Colon = :3
:3 + "prepend " = "prepend :3"
"prepend :3" + Prepend Hashtag = Prepend Hashtag :3
 "#);

    let improved_lineage = improve_lineage_depth_explorer(ass_lineage, &recipes_result_map, 1, 0).await;
    improved_lineage.print_lineages_ordered();
}









async fn do_punc_8() {
    // when this _auto_save goes out of scope, it saves 1 final time
    let _auto_save = auto_load_and_save_recipes(
        Duration::from_secs(30 * 60),
        "punc 8.json",
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



async fn do_alphabet_9() {
    // when this _auto_save goes out of scope, it saves 1 final time
    let _auto_save = auto_load_and_save_recipes(
        Duration::from_secs(30 * 60),
        "alphabet 9.json",
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

            "#),
        ..Default::default()
    };

    let encountered = depth_explorer_split_start(&de_vars).await;
    generate_lineages_file(&de_vars, encountered).unwrap();
}