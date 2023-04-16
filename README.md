# Helicoid
![Screenshot](./assets/helicoid_logo.svg)

Work in progress attempt at making a remote client for the Helix code editor. 
The code editor code is at the moment not used in the project as the focus is to get a client and the client 
server architecture basics up and running first.

Copyright: The goal is to license the code under MPL license to match the helix editor.
However the source code is based from the Neovide (https://github.com/neovide/neovide)
source code with is under MIT license.

Note however that the dependency on Glutin & Winit has been upgraded to the newest versions (post v0.30 rewrite).

The current state is that preliminary text shaping of single lines / paragraphs and window setup is working. 
So is the message passing between the server and client. Be ware there is currently no security 
for the network traffic and rkyv is running without validation (bytecheck) as that is not supported for complex enums.

The next step is to get the shaped text that is transmitted from the server to the client drawn using the block renderer.
This is almost complete.



Architecture:

Helicoid (a helix surface) aspires to be an editor frontend that enables the user to run the text editor on a remote system and 
access it as a local application. The goal is to be able to edit files that are stored on remote systems, or in 
virtual machines (or containers) fast and efficiently. It is also a hope that the frontend and editor can be combined
in one process for local editing when that is appropriate, but that a separation of drawing and layout and editing
still can be useful for testing and platform integration reasons.

The helicoid architecture is inspired by the Neovim architecture. The backend editor (from now called server), 
and the front end renderer (from now called client) communicates using an ordered, reliable byte stream (typically a TCP 
socket). All user input (keyboard, mouse, resizing etc.) is sent from the client to the server. The server
processes the input and lays out a renderable representation that is sent to the client. For interactivity, 
especially on high latency connections it is important to reduce round trips. Therefore only one round trip
should occur per input, and new inputs should not be dependent on that the previous output have arrived.

The display is based on a box model. All layout, including font shaping is done on the server to avoid round trips. 
This means that the font files needs to be accessable to the editor server. However the font shaping is performed
using the swash rust library, so no further UI dependencies (like Skia, OpenGL etc.) are neccesary on the server.

The text is shaped on the server on a per paragraph / shaping run basis. The shaping runs are organised in blocks
with a certain location and extent. There are also blocks for polygon based uis, nested blocks, and planned to 
support references to SVG/bitmap images. All the blocks are serialized using rkyv (for speed) and transfered
to the client to be drawed, see helicoid-protocol for the interface definitions and network logic.
It is intended for the client to reuse text blocks in new locations when the user
interface changes (e.g. to only send the affected paragraph when text changes). The server is expected to keep
track of what the blocks it has sent to the client contains for reuse and to request unused blocks to be removed
(garbage collected) at the client.

By making the user interface rendering flexible (compared to the character grid that NeoVim uses),
but still relatively simple it is a hope that more complex user interface concepts, like the ones used in 
Visual Studio Code and Jetbrains IDEs can be possible (like annotations with different font sizes). However
by requiring a round trip for all user interaction interactivity may suffer in some cases, but should 
likely not be much worse than an SSH terminal.


Future and notes:

I have been experimenting with this over the christmas holiday, now i will be back in a full job, so it is probably
limited how fast i can work on this. I hope still to be able to make something demoable rending the current text
interface of helix (at least one file/view) in a window within a few months, but if anyone wants to pick this up
that would still be very interesting to see how it turns out.

The code is quite messy (some stuff commented out, not really any tests etc.) as it is kind of in a prototype state
where the goal is to first get something simple up and running to see that it is feasable. If/when the prototype
is completed to a state where it is somewhat usable cleanup and better test coverage will likely be introduced.


Current plan:
- Finish/Get the possibility to render a document stored in helix-view to work, scrolling up and down the document.
- Look into making a key input system that can be (re) configured runtime based on some kind of serde (toml)
description.

How to use:
Enter the helicoid folder in a shell (where this readme file is) in two terminals. Build and run the server first, 
then the client. Which should open up a window. Currently this has only been tested on (arch)linux using wayland.

If you have problems with missing opengl functions while linking skia skia-safe should be patched with skia-safe.patch.

```
Server (enter build and run):
cd helicoid-testserver
cargo build
RUST_LOG=trace cargo run
; Press 'q' to exit (the message informing about this is printed using rust log)

Client (build in the helicoid folder):
cargo build
RUST_LOG=trace cargo run 
```
