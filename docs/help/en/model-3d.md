# 3D models

3D models appear in the grid next to the pictures, with a picture of the model as thumbnail. In the viewer the graphics card draws them, and you can turn them and look at them from every side.

![3D models](../img/model-3d.webp)

## Which models

- **STL**, **3MF** and **AMF** (3D printing), **OBJ** (with the colours of its materials, from the `.mtl` file next to it), **PLY** (also 3D scans; point clouds show as dots).
- **glTF** (`.glb`, and `.gltf` with its files), **FBX**, **Collada** (`.dae`, `.zae`), **3DS**, **OFF**, **VRML** and **X3D**.
- **IFC**: building models (walls, slabs, windows, structures) in their colours.
- **STEP**, **IGES** and **BREP**, the solid models of mechanical CAD programs. They need the CAD engine, downloaded once: the first one you open offers it, and so does the red **Download the CAD engine** button in the menu bar. Until then their thumbnail is a cube ![CAD model](icon://color-model) with a download sign. See [Add-on components](components.md).

A `.gltf` whose data file (usually `.bin`) is not next to it does not open: the viewer names the file that is missing. EyeCanSee reads that file only from the model's folder or from a folder inside it: when the model points to a folder above, to a whole path or to the internet, the viewer says so and names the file. A model whose data inside the file is damaged, or a `.glb` without its block of data, does not open either, and the viewer says which of the two it is. A `.glb` cut off, as after a download or a copy that stopped, does not open: the viewer says the file is incomplete and, when the file says how large it should be, gives that size and the one it has.

A model with very many triangles does not open. EyeCanSee opens binary PLY files of up to 50 million triangles; binary STL, `.glb`, `.gltf`, 3DS, OFF, Collada, VRML, X3D and FBX files of up to 20 million; OBJ, 3MF, text PLY and text STL files, which take more memory to read, of up to 10 million; AMF and IFC files of up to 5 million. Files of the CAD engine (STEP, IGES, BREP) go up to 50 million. In OBJ, 3MF, AMF, PLY, STL, 3DS and glTF (`.glb`, `.gltf`) files the triangles are counted before the model fills the memory, so a larger file is refused at once; the other formats are read before their triangles can be counted. A point cloud, whose points are each drawn as a small solid, opens up to 2.5 million points from a text PLY file and up to 12.5 million from a binary one. A larger model does not open, and the viewer says how many triangles its format allows. In the grid, in place of its thumbnail there is ![Too large for a thumbnail](icon://color-model) with the words **Too large for a thumbnail**.

The full list is in [Supported formats](formats.md).

## Looking at a model

Double-click a model, or select it and press **Enter**.

| Key or mouse | What it does |
|---|---|
| Arrows, or drag | Turn the model (hold an arrow to keep turning) |
| **Shift + drag**, or drag with the middle button | Move the model in the window |
| Mouse wheel, + and − | Zoom in and out |
| 0 | Back to the starting view |
| M | Measure |
| S | Section |
| Page Down, Space | Next file |
| Page Up, Backspace | Previous file |
| Home, End | First, last file |
| Esc, double click | Back to the grid |
| F11 | Full screen: the model alone |

The arrows turn the model, so they do not change file here. The **Keys** button in the status bar lists these keys; while Measure is on, the keys of Measure.

In the top right corner the axes of the file (X red, Y green, Z blue) turn with the model, as in CAD programs. The status bar shows how many triangles the model has, the zoom and, while it moves, how many frames a second are drawn.

## Measuring

**M**, or **View > Measure**, turns on the Measure tool: the status bar shows the overall size of the model along X, Y and Z, its surface and its volume. The volume is given only for a closed model (every edge shared by two triangles, as a model for 3D printing must be) whose faces all turn the same way; otherwise the status bar says how many edges are open, or how many edges lie between faces turned opposite ways. The size counts only the points of the triangles: points a file keeps without drawing them are left out. A file saved again under the same name is measured again when it is read again. Click two points on the model for their distance, written next to the pointer and in the status bar; a third click starts again. Measures are in the file's unit when the format or the file says it (glTF, IFC, X3D and VRML in metres, STEP and IGES in millimetres, 3MF and AMF as they say); STL, OBJ, PLY and others have no unit, and the status bar says so. **Copy** in the status bar, or **Ctrl + C**, puts the results on the clipboard; **Esc** clears the points, **Esc** again or **M** closes the tool. Dragging still turns the model.

## Section

**S** cuts the model with a plane, to see inside it: what is beyond the plane is left out, and where the plane crosses the solid it is filled in red, so walls and thicknesses stand out. The plane starts through the middle of the model, across Z for a model with Z up (for a building in IFC, a plan of the floor it crosses), otherwise across Y. The status bar says the axis and where the plane is, as a percentage (from one face of the model to the other) and as a distance from the first face, with a slider that moves it, marked every tenth of the model; or with the keys:

- **X**, **Y**, **Z**: the axis the plane goes across;
- **Alt** and the mouse wheel, **,** and **.**, or the slider in the status bar: move the plane;
- **F**: keep the other side;
- **S** again: the whole model.

While the plane moves, a see-through blue rectangle shows for a moment where it cuts, then fades. Closing the viewer takes the cut away: opened again, the model is whole.

**Save as…** saves the model with its section, as it is shown. The red fill is shown only for closed models (every edge shared by two triangles, as a model for 3D printing must be), whichever way their triangles face: a model with holes in its surface has no known inside, so its cut stays empty and the status bar says so.

## What is shown

The colours stored in the file are shown, and in many formats also the pictures on the surfaces (textures). A model without colours is light grey. The materials of glTF and FBX models, and of OBJ models (their `.mtl` file), also show how shiny or metallic a surface is, the light it gives (a lamp), and the small bumps of a normal map. Curved surfaces look smooth and edges stay sharp, also in models that do not store this information.

A glTF model that places one of its parts in more than one spot of its hierarchy, which the format does not allow, shows that part once, and the status bar says so. **Save as…** and **Export as a 3D model…** say it too: the new file has that part once.

Without a graphics card EyeCanSee can use, the processor draws the model, more slowly: it turns, zooms, is measured and cut the same way, and each new view comes after a moment.

## Saving

- **Ctrl + Shift + S**, or **Save as…** in the File menu or in the right-click menu of the viewer, saves the model as it is shown, as a picture, at least 1920 pixels on its long side. See [Save as](save-as.md).
- **Export as a 3D model…** saves it in another 3D format. See [Exporting 3D models](model-export.md).

Turning and flipping from the Image menu do not apply to models: a model turns with the mouse and the arrows.
