# Blender scripts

`frost_bike.py` is the Blender half of Frost Studio's bike builder. Studio writes it next to
each job and runs the rider's own, separately installed Blender on it:

    blender -b --factory-startup --python-exit-code 1 --python frost_bike.py -- job.json

It imports `bpy`, so it is licensed **GPL-3.0-or-later**, as Blender asks of scripts that do
(see its SPDX header and <https://www.gnu.org/licenses/gpl-3.0.html>). That licence covers
this folder only. The rest of the repository keeps its own licence. Studio neither ships nor
links Blender, and what a build exports (FBX, GLB) belongs to whoever built it.
