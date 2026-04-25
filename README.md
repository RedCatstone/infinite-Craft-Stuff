This includes 3 things:
- Depth/Layer Explorer - from a starting seed find all N-step away elements
- loading/merging/saving recipes from/to `Infinite Craft .ic`-format and `recipesNum.json`-format (and old `depth_explorer.json`-format)
- Lineage Generator (not very good right now, but will improve)



# To get it running:
1. install rust - https://rust-lang.org/tools/install/
2. clone the repository
```
git clone https://github.com/RedCatstone/infinite-Craft-Stuff
```
```
cd rust
```
3. this does not have a fancy ui, so you will have to modify main.rs to do pretty much anything. There are a bunch of example functions tho!
4. run the release version (debug is simply too slow for recipe file loading...)
```
cargo run --release
```

5. if you want the code to do actual requests, setup a "combination-proxy" server **YOURSELF** which does:
`http://localhost:3000/?first=Fire&second=Water` -> `{ result: ..., emoji: ..., isNew: ... }`  
(forwarding a request directly from https://neal.fun/infinite-craft/)
> [!WARNING]  
> this repo does not include the setup for that proxy.
