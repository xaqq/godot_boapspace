# Feature

We want to add Skills to NPCs.
Skills can be understood as "skill level" in specific field for a NPC.


## Skills

### Existing Skills

We want the following Skills:
  + Builder
  + Farmer
  + For each existing resources make an attribute that match the skill (food -> food gatherer, etc.) You can use actual
    profession name.

### Skills value.

#### Numerical 

Skills values have range between 0 (never done that activity) to 10000 (is an expert).

#### Textual

We want enumeration representation for skills level. Like Journeyman, Expert, Master etc.
/!\ Help me refine that. Propose bunch of relevant names to use.


#### Gaining skill level.

When an entity perform an action, such as gathering, then the skill level should go up by one.

## UI impact

The NPC info panel will be too crowded if we add Skills information there.
What we need a new dedicated panel for NPCDetails; Similar to the Task panel in the way it fit into the UI.
To open that NPC details panel, a button should be added to the currently existing npc selection panel.

### NPC Details Panel Content

It should display all information available about a NPC. This includes what is current being shown in the 
selected-npc-panel but should in addition show information about each of the Skills.

### Skills display

It should be easy to see and gauge the value of each attribute, so for each attribute we want to see:
  + A percentage value (rounded up or down, but not decimal number).
  + A progress bar that help visualize this.
  + The "textual" skill level.

Feel free to layout that is a nice way.

## Notes

Skill level have no impact on activity at this point.