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





#[tokio::main]
async fn main() {

    // --- LOAD RECIPES ---
    // there are 3 load_recipes_xxx functions. if multiple recipe files are loaded, it merges them

    // load_recipes_num("D:\\InfiniteCraft\\Codes\\recipesNum.json");
    // load_recipes_old_depth_explorer("D:\\InfiniteCraft\\Codes\\recipes.json");
    // load_recipes_gzip("./Recipe Files Out/full_db.ic").expect("a");


    // v analyzer format!!! v
    // save_recipes_gzip("full_db.ic", "Full Db").expect("could not save...");

    do_punc_8().await;



    // load_recipes_num(&format!("{}/depth_explorer_recipes.json", SAVED_RECIPES_FILES_LOCATION));


    // let recipes_result_map = get_recipes_result_map();
    // let recipes_uses_map = get_recipes_uses_map();
    // let mut heuristic_map = get_element_heuristic_map(&recipes_uses_map);


    // let punc_alts = generate_lineage_multiple_methods(&["Punctuation", "Alphabet", "Delta"], &mut heuristic_map, &recipes_result_map, &recipes_uses_map, false);
    // punc_alts.print_lineages_ordered();

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
    // let improved_lineage = improve_lineage_depth_explorer(ass_lineage, 1, 0).await;
    // improved_lineage.print_lineages_ordered();




    // lineage stuff
    
    // init_heuristic();
    // let binding = [str_to_num_fn("Cat")];
    // let lineage = generate_lineage(&binding, 1);
    // println!("{}", format_lineage(lineage));
    // let lineage = remove_unneccessary(generate_lineage(&binding, 1));
    // println!("{}", format_lineage(lineage));
}






async fn do_punc_8() {
    let auto_load_and_save_file = "depth_explorer_recipes - Punc 8.json";
    load_recipes_num(&format!("{}/{}", SAVED_RECIPES_FILES_LOCATION, auto_load_and_save_file));
    // load_recipes_gzip("D:/InfiniteCraft/Codes/rust/Recipe Files In/Helper-Save (May 19, 2025, 07-47 PM).ic").expect("could not load");

    let auto_save = auto_save_recipes(Duration::from_secs(30 * 60), || {
        println!("saving recipes...");
        save_recipes_num(auto_load_and_save_file).expect("could not auto save...")
    });

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

    rerequest_all_nothing_recipes().await;

    // let encountered = depth_explorer_split_start(&de_vars).await;
    auto_save.save_now();
    // generate_lineages_file(&de_vars, encountered).expect("could not generate lineages file...");
}