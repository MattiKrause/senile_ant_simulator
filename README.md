# Senile Ant Simulator
This is a model of ant food foraging and swarm behavior. This project was inspired by [Sebastian Lague](https://www.youtube.com/watch?v=X-iSQQgOd1A)
## General Concepts
Even though there are multiple frontends the core simulation is always the same.
It consists of rectangular board which is made up of cells. Cells can have on of four types
* Path
* Food
* Blocker
* Home

The goal of the ants is to find their way to a food resource harvest a bit of that
resource and bring that bit back home. All cells except of blocker cells can be traversed by the ants.
To help them navigate, ants leave a pheromone trail, which gradually decays. There are two types of pheromones, the home pheromone, 
which leads from the hive to the food and the food pheromone which leads from the food to the home.
The ants follow different pheromones depending on their destination, but there is also a bit of randomness 
in their behavior to allow them to explore their environment to find new resources or more efficient paths.
Overall this should lead to "ant highways" forming between food and home. Currently, the ants are not able to do that
which is why this project is named senile ant simulator.
## How to use
This project currently be accessed in two ways:
* The frontend_recording, which can be used from the command line to produce a gif of the simulation. See [Recording Frontend](#Recording Frontend) 
* The eframe_frontend, which allows viewing and editing the simulation in a gui. Works on Desktop and Web. See [GUI Frontend](#GUI Frontend)
### Recording Frontend
The recording frontend allows you to record the ant simulation.
You can compile it from source by installing [Rust](https://github.com/rust-lang/rust) and running
```shell
cargo build --release -p=frontend_recording
```
To run it use
```shell
./target/release/frontend_recording --save_file <save_file> --gif <target_file> --time_limit <time_limit>
```
The time limit sets the desired length of the gif in seconds. The `--time_limit` argument is optional, 
but without the program must be manually killed, for example by using `CTRL + C`\
Another optional argument is `--delay` which controls the delay between frames in milliseconds. 
Due to constrains of the gif format, the delay can only be set in increments of 10.\
To get more help use `--help`.

### GUI Frontend
#### Running the code
The gui frontend allows you to view the simulation and alter the board.\
the gui can be accessed on desktop and web:\
To access the app on web go to https://mattikrause.github.io/senile_ant_simulator/ \
To compile it from source first install [Rust](https://github.com/rust-lang/rust).\
To compile the code for your current desktop version run:
```shell
cargo build --release -p=eframe_frontend
```
To compile the code for web, first install [trunk](https://trunkrs.dev/),
then run
```shell
cd eframe_frontend
trunk build --release
```
Then open `eframe_frontend/dist/index.html` in your web browser.
If you use firefox or want to run the code more easily run
```shell
cd eframe_frontend
trunk serve --release
```
And open the link in console.
#### Features
To load a file in a non-web environment CTRL+L which should result in a file dialog opening, 
then select the file. A simple file is available under `ant_sim_saves/ant_sim_test_state.txt`.\
To save a file in a non-web environment press CTRL+S which should result in a file dialog opening. 
To save in a web Environment first press the File button in the upper left, then Save.\
Otherwise a file can be loaded by dragging it onto the window. This feature is currently not available 
for users of Linux with Wayland due to a known bug in a dependency.\
Loading a file sets the app to edit mode, which allows you to:
* Set the height and width of the board
* Set the seed which controls the randomness in the ant behavior.
* Paint the map using a brush, whose radius can be controlled using the width setting and whose painted brush kind can be changed by pressing:
  * C for clear
  * B for blocker
  * H for home
  * F for food
  * A for ants

The game can be launched using the start butting on the left.
The game speed is displayed at the top right and can be set using the keys 0-9 and p,
where p pauses the game and 0 sets the game speed to fastest. 
## Technical

The project is split into multiple crates by responsibility:
* the top level crate is concerned with runtime data representation and domain specific computations like the state update function
* the rgba_adapter crate is concerned with the turning the data into an image, since that functionality is shared among multiple crates
* the ant_sim_save crate is concerned with persistent data representation and saving the runtime data to memory
* the recorder crate is concerned with saving the image data to a video format since that functionality can be used by multiple crates 
* the frontend_recording crate is concerned with providing a cli to the recorder crate
* the eframe_frontend crate is concerned with providing a gui to a user

### Ant sim save
the ant_sim_save crate is concerned with a persistent data representation, 
since the runtime data is implementation specific. And the requirements of the persistent and runtime representation differ significantly.
While runtime data must be fast to operate on, persistent data must be compact, small and if possible backwards compatible. 

### Recorder
The recorder crate is currently mainly an interface to the gif library used to write the data.
One thing that makes the crate more complex is that it translates the image color data into the gif appropriate color format,
since the methods provided by the gif library are too slow and produce visible  artifacts in the resulting gif.

### eframe frontend
The eframe frontend uses the [eframe](https://github.com/emilk/egui/tree/master/crates/eframe)/[egui](https://github.com/emilk/egui) infrastructure to develop the app.
egui was chosen for its simplicity, easy of use and portability, even though it's simplicity sometimes gets in the way, 
for example reusing image buffers is not possible.

Other option were evaluated like, pixels(which did not provide an easy way to make gui), luminance(which was too complicated), iced(which did not provide a dropped file event).

The architecture of the frontend is based on the actor model. All changes are send to the app actor, which changes 
the app state appropriately. This approach allows the usage of async rust by making the actors async, 
and it makes working with the borrow checker much easier. Other actors include a simulation computation actor, 
a simulation update broker(which adds as a broker between the computation actor and the app actor), and a file actor(which handles dropped files and file dialogs).
