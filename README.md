# Rvim

This is my personal reimplementation of Vim in Rust. This is primarily just an
exercise, but my goal is for it to be good enough to use as my regular editor

I would like to minimize external dependencies as much as possible. My goal is
to eventually have only the following dependencies:

- libc
- Unicode segmentation
- Treesitter when using LSP
- (Potentially) Parser for faster syntax highlighting, command parsing

## Features Needed for usability

 - [X] Basic motions
 - [ ] Handle Unicode and malformed input
 - [ ] Proper Command syntax
 - [ ] Nice error reporting
 - [ ] Efficient Rope implementation
 - [ ] Undo
 - [ ] Temporary gap buffer during inserts
 - [ ] Swap file for autosave
 - [ ] Syntax Highlighting
 - [ ] Autocomplete snippets
 - [ ] LSP support

 ## Neat things

 ### Rope

 Text editor data structures are quite a bit of fun to create. Right now, I'm
 using a [rope][rope], with some minor modifications. Simply put, a rope is a
 binary tree where each node has a weight equal to the length (of text) if its
 left subtree. If the node is a leaf, then its weight is simply the length of
 its contained string. Virtually all operations on a rope can be done using
 only split and merge operations.

 My implementation has a few modifications. For one, I use immutable, reference
 counted strings and a byte range. This allows for less copying when inserting
 into the middle of a leaf, and for potential future undoing support. I also add
 the constraints that a leaf node must either end with a newline, or contain
 zero newlines. This lets me store the number of newline characters in each
 node.

 Here is a debug print of a rope in its current iteration that represents the
 string `ab---cd` (2023-07-08):
 ```
 Rope {
    lf_cnt: 0,
    weight: 2,
    inner: NonLeaf {
        left: Rope {
            lf_cnt: 0,
            weight: 2,
            inner: Leaf {
                content: "ab",
            },
        },
        right: Rope {
            lf_cnt: 0,
            weight: 3,
            inner: NonLeaf {
                left: Rope {
                    lf_cnt: 0,
                    weight: 3,
                    inner: Leaf {
                        content: "---",
                    },
                },
                right: Rope {
                    lf_cnt: 0,
                    weight: 2,
                    inner: Leaf {
                        content: "cd",
                    },
                },
            },
        },
    },
}
 ```

 While this implementation is neat, it's still imperfect and needs some future
 improvements. As with a vanilla binary search tree, my rope implementation is
 susceptible to becoming unbalanced. Since a lot of text is added one character
 at a time, one after another, my rope may quickly become a linked list. I wish
 to solve this by utilizing a [gap buffer][gapbuffer] for individual insertions
 and persisting it as an entry in the rope. Binary trees also tend to have poor
 spacial locality, so I'm interested in converting the rope into a from a binary
 tree to a B-tree-like structure. In addition to improving spacial locality of
 nodes, it will also completely solve bad balancing issues.

 [rope]: https://en.wikipedia.org/wiki/Rope_(data_structure)
 [gapbuffer]: https://en.wikipedia.org/wiki/Gap_buffer

 ### Tui rendering

 I'm not using any terminal rendering libraries such as
 ncurses, so I need to rediscover a lot of techniques for rendering TUIs. There
 are two fundamental problems:

 1. A `char` may not necessarily be one cell
 2. Modern terminals are super slow.

I haven't fully figured out solutions to these, but I will update this readme
with what I find.

