# RE Engine Package Tools

A high-performance Rust implementation for quickly unpacking RE Engine game packages.

Structural analysis and algorithms are derived from project [Ekey/REE.PAK.Tool](https://github.com/Ekey/REE.PAK.Tool). Thanks for your work!

The unpack tool needs file list to work (`.list` files), you can get them here: [https://github.com/Ekey/REE.PAK.Tool/tree/main/Projects](https://github.com/Ekey/REE.PAK.Tool/tree/main/Projects), and put them in `assets/filelist` folder.

## CLI Version

Command line interface version.


```
Usage: ree-pak-cli.exe <COMMAND>

Commands:
  unpack     Unpack a PAK file
  dump-info  Dump PAK information
  pack       Pack files into a PAK file
  help       Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
```

### Unpack

Unpack a `PAK` file. The process is like unpacking a `ZIP` file, but with some additional features.

```
Usage: ree-pak-cli.exe unpack [OPTIONS] --project <PROJECT> --input <INPUT>

Options:
  -p, --project <PROJECT>  Game project name or list file path, e.g. "MHRS_PC_Demo", "./MHRS_PC_Demo.list"
  -i, --input <INPUT>      Input PAK file path
  -o, --output <OUTPUT>    Output directory path
  -f, --filter <FILTER>    Regex patterns to filter files to unpack by file path [default: ]
      --ignore-error       Ignore errors during unpacking files
      --override           Override existing files
      --skip-unknown       Skip files with an unknown path while unpacking
  -h, --help               Print help
```

### Pack

Pack files and folders into a `PAK` file. The process is like packing a `ZIP` file, but with some additional features.

You should ensure that the input directory is well organized, the packer will locate the *start of path* and preserve the *internal* directory structure.

```
Usage: ree-pak-cli.exe pack [OPTIONS] --input <INPUT>

Options:
  -i, --input <INPUT>    Input directory path
  -o, --output <OUTPUT>  Output PAK file path
      --override         Override existing file
  -h, --help             Print help
```

#### Behavior

For example, if you have the following directory structure and input directory is `MyExcellentMod`:

`MyExcellentMod/natives/STM/streaming/Art/Model/Character/ch03/002/001/2/textures/ch03_002_0012_ALBD.tex.241106027`

Then the packer will locate the `natives/` and trim any path before it, resulting in a file path like:

`natives/STM/streaming/Art/Model/Character/ch03/002/001/2/textures/ch03_002_0012_ALBD.tex.241106027`

If `natives/` not found, tool will issue a warning.

#### Tips

If you have only one input to pack, you can drag-and-drop the folder onto the executable file. The tool will generate a output pak file with default name and options.

### Dump Info

Dump information of a `PAK` file. The output is a JSON file that contains the file TOC, entries and file paths.

In other words, the output contains all the data except the actual file contents, even including unknown fields.

```
Usage: ree-pak-cli.exe dump-info [OPTIONS] --project <PROJECT> --input <INPUT>

Options:
  -p, --project <PROJECT>  Game project name, e.g. "MHRS_PC_Demo"
  -i, --input <INPUT>      Input PAK file path
  -o, --output <OUTPUT>    Output file path
      --override           Override existing files
  -h, --help               Print help
```

## GUI Version

The GUI version provides a visual tree view that allows you to extract the specified files.

Note that the GUI version can only **unpack** files, the packaging function is on the roadmap.

See sub project [eigeen/ree-pak-gui](https://github.com/eigeen/ree-pak-gui).

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
