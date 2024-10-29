# RE Engine Package Tools

A high-performance Rust implementation for quickly unpacking RE Engine game packages.

Structural analysis and algorithms are derived from project [Ekey/REE.PAK.Tool](https://github.com/Ekey/REE.PAK.Tool). Thanks for your work!

The unpack tool needs `.filelist` files, you can get them here: [https://github.com/Ekey/REE.PAK.Tool/tree/main/Projects](https://github.com/Ekey/REE.PAK.Tool/tree/main/Projects), and put them in `assets/filelist` folder.

## GUI Edition

See sub project [eigeen/ree-pak-gui](https://github.com/eigeen/ree-pak-gui).

## CLI Edition

stub

## Benchmarks

Tested on my PC for reference.

Test file: MHRS Demo

> RETool
> 
> Time: 249 s

> [REE.Unpacker](https://github.com/Ekey/REE.PAK.Tool) 20240921
> 
> Time: 84 s

> REE.Unpacker (No Logging) 20240921
> 
> Time: 76 s

> MHRUnpack v1.2
> 
> Time: 218 s (Single Thread)
> 
> Time: 136 s (Multi Thread)
> 
> High CPU usage, but not very fast.
> Has GUI.

> [ree-pak-cli](https://github.com/eigeen/ree-pak-rs) v0.1.0
> 
> Time: 29 s
