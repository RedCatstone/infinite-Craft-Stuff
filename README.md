This includes 3 things:
- Depth Explorer (from a starting seed find all N-step away elements)
- Lineage Generator (not very good right now, but will improve)
- loading/merging/saving recipes from/to `Infinite Craft .ic`-format and `recipesNum.json`-format (and old `depth_explorer.json`-format)



# To get it running:
1. install rust and all its dependencies
2. clone the repository
```
git clone https://github.com/RedCatstone/infinite-Craft-Stuff/tree/master
```
3. a simple `cargo run --release` does the rest!
   (all settings you need are found in the main.rs file)

4. if you want the code to combine stuff, setup a "combination"-proxy server which does:

`http://localhost:3000/?first=Fire&second=Water` -> `{ result: result_text, emoji: result_emoji, isNew: result_isNew }`
(basically just forwarding a request directly from `https://neal.fun/infinite-craft/api/infinite-craft/pair?first=Fire&second=Water`
> [!WARNING]  
> this repo does not show you how you can set this up.
