#![allow(dead_code)]

mod structures;
mod recipe_loader;
mod lineage;
mod depth_explorer;
mod recipe_requestor;
mod layer_explorer;

use std::io;

use crate::depth_explorer::DepthExplorerVars;
use crate::layer_explorer::LayerExplorer;
use crate::recipe_loader::RecipesFile;
use crate::structures::{Element, RecipesState, UNKNOWN_ID, sort_recipe_tuple};




/// just leave this to true
const LINEAGES_FILE_COOL_JSON_MODE: bool = true;

/// where all files are located that this code will access.
/// this is relative to Cargo.toml file.
/// you can also use a full path instead of a relative one
const RECIPE_FILES_FOLDER: &str = "Recipe Files";

/// to make the code faster it uses constant sized ``ArrayVecs``, meaning that they
/// can't grow longer than this number:
const DEPTH_EXPLORER_MAX_STEPS: usize = 10;

/// only for ancient code
const DEPTH_EXPLORER_JUST_MARK_UNKNOWN_NO_REQUESTS_NO_ENCOUNTERED: bool = false;
/// only for ancient code
const DEPTH_EXPLORER_DEPTH_GROW_FACTOR_GUESS: usize = 15;



#[tokio::main]
async fn main() {
    // --- LOAD RECIPES ---
    // there are 3 formats. if you load multiple recipe files, it simply merges them
    
    // -- my own formats --
    // RecipeFileFormat::JSONOldDepthExplorerRecipes - ancient format from my first version (`ing1=ing2=result`): 
    // RecipeFileFormat::JSONRecipesNum - (has a numId_to_str array at the top, and then recipes listed with their ids)

    // -- .ic format --
    // RecipeFileFormat::ICSaveFile
    // state.load("full_db.ic", recipe_loader::RecipeFileFormat::ICSaveFile).unwrap();

    // you can comment this panic out
    panic!("please look at src/main.rs and change what you need! (you can comment this panic out over there)");


    // fill_in_recipes().unwrap();
    // test_layer_explorer().await;
    // requests_go_brr("13_missing_recipes_batch0.ic", RecipesFile::ICSaveFile).await;
}


fn keep_only_unknown(name: &str) {
    let mut all = RecipesState::without_autosave();
    all.load(name, RecipesFile::ICSaveFile).unwrap();
    all.remove_recipes_not_resulting_in(&[UNKNOWN_ID]);
    all.save(name, RecipesFile::ICSaveFile).unwrap();
}


fn fill_in_recipes() -> io::Result<()> {
    // these are all the files you want to fill it with. (should be in Recipe Files folder, next to src/.)
    // you can simply change the loads here
    let mut state = RecipesState::without_autosave();
    state.load("depth_explorer_recipes.json", RecipesFile::JSONRecipesNum)?;
    state.load("alphabet 9.json", RecipesFile::JSONRecipesNum)?;
    state.load("punc 8.json", RecipesFile::JSONRecipesNum)?;
    state.load("more than Punc 8.json", RecipesFile::JSONRecipesNum)?;
    state.load("scorpia fill.ic", RecipesFile::ICSaveFile)?;
    // ...
    
    // this is the recipe file that contains all of the `=UNKNOWN=` recipes.
    // also should be in the Recipe Files Folder, next to src/.
    let mut unknowns = RecipesState::without_autosave();
    unknowns.load("13_missing_recipes.ic", RecipesFile::ICSaveFile)?;
    unknowns.fill_unknowns_with(&state);

    // now we will save a file with only the new filled in recipes!
    unknowns.remove_recipes_resulting_in(&[UNKNOWN_ID]);
    unknowns.save("13_missing_recipes_updated.ic", RecipesFile::ICSaveFile)?;

    Ok(())
}














pub fn calc_depth_13() {
    let mut state = RecipesState::without_autosave();
    state.load("from_base 13.json", RecipesFile::JSONRecipesNum).unwrap();

    let lineage_elems: Vec<Element> = state.string_lineage_results(true, "");
    let max_steps = 13;
    
    LayerExplorer::start(&state, &lineage_elems, max_steps, true, true);

    state.extract_to_request().save("13_missing_recipes.ic", RecipesFile::ICSaveFile).unwrap();
}



pub async fn test_layer_explorer() {
    let mut state = RecipesState::without_autosave();
    state.load("13_missing_recipes.ic", RecipesFile::ICSaveFile).unwrap();

    let lineage_elems: Vec<Element> = state.string_lineage_results(true, "");
        // Earth + Wind = Dust
        // Water + Dust = Mud
        // Earth + Dust = Planet
        // Fire + Mud = Brick
        // Brick + Planet = Mars
        // Brick + Brick = Wall
        // Earth + Mars = Life
        // Earth + Life = Human
        // Mars + Life = Alien
        // Wall + Life = Prison
        // Alien + Human = Hybrid
        // Life + Prison = Sentence
        // Hybrid + Sentence = Hyphen

    let max_steps = 9;
    
    LayerExplorer::start_step_by_step_with_requests(
        &mut state, &lineage_elems, max_steps, true, false
    ).await;
}



pub async fn requests_go_brr(file_name: &str, file_mode: RecipesFile) {
    let mut state = RecipesState::with_autosave(file_name, file_mode, 500_000);
    state.load(file_name, file_mode).unwrap();
    state.request_all_unknown_recipes().await;
    state.rerequest_all_nothing_recipes().await;
    // state.rerequest_all_nothing_recipes().await;
}







// --- OLD CODE ---
// everything below this point is using the old depth explorer


async fn test_depth_explorer(state: &mut RecipesState) {

    // rerequest_all_nothing_recipes().await;

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_STEPS,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 2,
        lineage_elements: state.string_lineage_results(true, r#"


            "#),
        exclude_depth1_elements: vec![],
        ..Default::default()
    };

    let encountered = state.depth_explorer_split_start(&de_vars).await;
    // state.save("from_base_depth13_unknowns.json", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();
    state.generate_lineages_file(&de_vars.lineage_elements,  de_vars.stop_after_depth, &encountered).expect("could not generate lineages file...");
}




async fn test_caps(state: &mut RecipesState) {
    let recipe_tup = sort_recipe_tuple((state.str_to_num_fn("Rocket").unwrap(), state.str_to_num_fn("Cloud").unwrap()));
    let result_num = *state.recipes_ing.get(&recipe_tup)
        .expect("'Cloud + Rocket' is not in recipes_ing");

    println!("result: {result_num} {}", state.num_to_str_fn(result_num));
    
    state.to_request_recipes.insert(recipe_tup);
    

    println!("{:?}", state.process_all_to_request_recipes("Test Caps").await);


    let recipe_tup = sort_recipe_tuple((state.str_to_num_fn("Rocket").unwrap(), state.str_to_num_fn("Cloud").unwrap()));
    let result_num = *state.recipes_ing.get(&recipe_tup)
        .expect("'Cloud + Rocket' is not in recipes_ing");

    println!("result: {result_num} {}", state.num_to_str_fn(result_num));
}










async fn test_lineage_stuff(state: &mut RecipesState) {
    // --- LINEAGE GENERATION STUFF ---

    // recipe_loader::load("depth_explorer_recipes.json", recipe_loader::RecipeFileFormat::JSONRecipesNum).unwrap();
    // let _auto_save = auto_load_and_save_recipes(
    //     Duration::from_secs(30 * 60),
    //     "depth_explorer_recipes.json",
    //     recipe_loader::RecipeFileFormat::JSONRecipesNum
    // );

    let recipes_result_map = state.get_recipes_result_map();
    let recipes_uses_map = state.get_recipes_uses_map();
    let mut heuristic_map = state.get_element_heuristic_map(&recipes_uses_map);

    state.generate_lineage_multiple_methods(&["Unova Cap Pikachu"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, true);


    let punc_alts = state.generate_lineage_multiple_methods(&["Punctuation", "Alphabet", "Delta"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, false);
    punc_alts.print_lineages_ordered(state);

    let ass_lineage = state.string_lineage_to_lineage(r#"
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

    let improved_lineage = state.improve_lineage_depth_explorer(ass_lineage, &recipes_result_map, 1, 0).await;
    improved_lineage.print_lineages_ordered(state);
}









async fn do_punc_8(state: &mut RecipesState) {
    // when this _auto_save goes out of scope, it saves 1 final time
    // let _auto_save = auto_load_and_save_recipes(
    //     Duration::from_secs(30 * 60),
    //     "punc 8.json",
    //     recipe_loader::RecipeFileFormat::JSONRecipesNum
    // );

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_STEPS,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 2,
        lineage_elements: state.string_lineage_results(true, r#"

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

    let encountered = state.depth_explorer_split_start(&de_vars).await;
    state.generate_lineages_file(&de_vars.lineage_elements, de_vars.stop_after_depth, &encountered).unwrap();
}



async fn do_alphabet_9(state: &mut RecipesState) {
    // when this _auto_save goes out of scope, it saves 1 final time
    // let _auto_save = auto_load_and_save_recipes(
    //     Duration::from_secs(30 * 60),
    //     "alphabet 9.json",
    //     recipe_loader::RecipeFileFormat::JSONRecipesNum
    // );

    let de_vars = DepthExplorerVars {
        stop_after_depth: DEPTH_EXPLORER_MAX_STEPS,  // modify the global variable, so the compiler knows how big stuff is gonna be -> SPEEEEED
        split_start: 2,
        lineage_elements: state.string_lineage_results(true, r#"

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

    let encountered = state.depth_explorer_split_start(&de_vars).await;
    state.generate_lineages_file(&de_vars.lineage_elements, de_vars.stop_after_depth, &encountered).unwrap();
}