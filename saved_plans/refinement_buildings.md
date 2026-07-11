# Refinement buildings

## Summary

Currently, we have raw resources, Food, Wood, Stone, Gold.
The intent is to have more "detailed" resources, for example:
  + Crop for food gathered from farm.
  + Food resource node maybe should yield/become "wild berries" or something like this.

We then want to have refinement building such as:
  + Woodworker (help me find a better name) that would take wood and yield Plank.
  + StoneWorker (help me find a better name) that convert stone into StoneBlock or something like this.
  + Kitchen that takes either crop or wild berries and make food.

We want to add relevant NPC skills for refining resources.
We want new assets for the new buildings and the new resources.

## Updating building cost.

Existing building code should have their wood/stone cost changed
to plank/stoneblock cost.
The kitchen should cost plank/stoneblock.
The WoodWorker and StoneWorker building should still cost raw resource.
