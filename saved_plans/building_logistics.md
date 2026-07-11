# Building logistics

We want user to be able to configure / control some element of building logistics.
We already have some stuff with warehouse resource whitelist.

## New small-warehouse and current warehouse change

We want to introduce a smaller warehouse. Maybe we call it a Dump or something else. Recommend me a few names.
It should have a 2x2 footprints and cost wood and stone. It works exactly as the warehouse but has a smaller inventory.
The current warehouse building should have its footprint extended to 4x4.

We need new assets for that new smaller warehouse.

## General control

Refinement building and warehouse should be able to be active or inactive.
An inactive building should basically be ignored by existing game system (refinement building don't produce generate
task for production). An inactive warehouse cannot accept new material and cannot be pulled from.

## Building name

User should be able to give explicit name to their building, to easily identify them.
Building control UI should allow a text field to name buildings.

Building name should:
  + Be unique across an entire surface
  + Have sensible default (BUILDING_NAME #BUILDING_NUMBER)
  + Visible on the map, hovering over buildings.

## Warehouse

Warehouse should be able to explicitly pull resources from refinement building.
In the control UI, it could be an additional checkbox per resources.

Warehouse worker pulling resources into the warehouse have wheelbarrow.

Wheelbarrow extend the cargo capacity of the NPC. It should be kept simple, so it can be a simple component
attached to the NPC entity that behave similarly to the NPC cargo component.

When NPC is done hauling to the warehouse, it should lose its wheelbarrow component.


### Wheelbarrow assets

Wheelbarrow should be visible to the user.
We need animated assets for wheelbarrow, both when empty and when carry resources. We want
different assets for when the wheelbarrow is carrying different type of resources.

## Production / refinement building.

Similar to warehouse, production building should be able to pull from warehouse.
In practice, this is a pull from a warehouse, so a NPC hauling would have access to wheelbarrow as they exit the 
warehouse with the resources they are hauling.
The wheelbarrow can magically disappear when the haul is done.


## Impact on Tasks.

Currently, we have task spawned by refinement building. As far as I know working those tasks includes
gathering the raw resource and hauling it. This will probably need to be changed: Refinement task should be created
when the building input buffer is not empty and when its buffer is not full.
If the input buffer is not full, task should be generated for gathering the resource and hauling it the building.
If "pull" is enabled on the building, hauling task should be generated and should not generate "collect raw resource" activity.


# Building mouse over overlay

Remove the footprint and cell information from this.
Instead, show either its inventory (for warehouse buildings) or its input/output buffer for production building.
A small recap of building control should also be displayed (is "pull" enabled, and for which resources.)
