# Feature

We want to add Farming to the game.

## Building

### Farm

We need a new Farm building. It can have a 3x3 footprint.
A farm on its own is useless. We need to assign Fields for farming.

Cost: 20 wood 30 stone

### Fields

Field must be adjacent to the farm, or to another fields linked to that farm.
There is a limit of 200 field per farm.
A field is basically a 1x1 building.

/!\ Help me refine this.
A Field can have multiple simple state:
+ Inactive: Until the farm has been built
+ Seedable: Field plot ready for seeding.
+ Growing_Step_1
+ Growing_Step_2
+ Grown: Crop ready for gathering

Growing step1 and 2 are just here so we can have more varied visuals.

Cost: 5wood 1 stone.

## Production

When a farm (its fields) have all been seeded, it takes 1 year in game for the crop to be grown.
A grown crop should behave like a Food resource node, but gathering it should give Farmer skill.


## AI interaction

### New behavior

When a farm is ReadyForSeeding (it has been built and all its fields are also built), a system should
generate a Task for each field that describe the intention of seeding it. Something like TaskSeedField.
Similarly, when crops are grown, a GatherCrop or similar should be created.

A new Farmer tag-component should be created (and added to the NPC bundle). A system should consider
farmer and send them to seed the fields. Similar for gathering crops.

### Seeding time

It takes 1 in game day to seed a field plot.

## Assets

Generate new assets as needed.


### UI impact.

Putting blueprint down for field plot should be different than for normal building.
Since its a 1x1 tile, it would be annoying to manually click up to 200 time for a big farm.
A mechanism with holding mouse and "dragging" would be nice.
