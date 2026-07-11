#!/usr/bin/env python3
"""Make cardinal connections in the generated road atlases seamless.

The interiors remain generated artwork. Only the outer edge bands of connected
arms are replaced with complementary halves of canonical strips sampled from
each tier's horizontal and vertical straight tiles.
"""

from __future__ import annotations

import argparse
from pathlib import Path

from PIL import Image


TILE_SIZE = 64
ATLAS_GRID_SIZE = 4
PORT_DEPTH = 8
NORTH = 1
EAST = 2
SOUTH = 4
WEST = 8

DEFAULT_ATLASES = (
    Path("godot/assets/generated/road_dirt_path_atlas.png"),
    Path("godot/assets/generated/road_cobblestone_atlas.png"),
    Path("godot/assets/generated/road_flagstone_atlas.png"),
)


def tile_box(mask: int) -> tuple[int, int, int, int]:
    x = (mask % ATLAS_GRID_SIZE) * TILE_SIZE
    y = (mask // ATLAS_GRID_SIZE) * TILE_SIZE
    return x, y, x + TILE_SIZE, y + TILE_SIZE


def strongest_vertical_seam(tile: Image.Image) -> int:
    """Find a central x boundary with the most opaque material crossing it."""
    alpha = tile.getchannel("A")
    candidates = range(TILE_SIZE // 4, TILE_SIZE * 3 // 4)
    return max(
        candidates,
        key=lambda x: sum(
            min(alpha.getpixel((x - 1, y)), alpha.getpixel((x, y)))
            for y in range(TILE_SIZE)
        ),
    )


def strongest_horizontal_seam(tile: Image.Image) -> int:
    """Find a central y boundary with the most opaque material crossing it."""
    alpha = tile.getchannel("A")
    candidates = range(TILE_SIZE // 4, TILE_SIZE * 3 // 4)
    return max(
        candidates,
        key=lambda y: sum(
            min(alpha.getpixel((x, y - 1)), alpha.getpixel((x, y)))
            for x in range(TILE_SIZE)
        ),
    )


def horizontal_port_bands(tile: Image.Image) -> tuple[Image.Image, Image.Image]:
    """Return complementary west/east bands from a continuous source strip."""
    seam = strongest_vertical_seam(tile)
    strip = tile.crop((seam - PORT_DEPTH, 0, seam + PORT_DEPTH, TILE_SIZE))
    east_band = strip.crop((0, 0, PORT_DEPTH, TILE_SIZE))
    west_band = strip.crop((PORT_DEPTH, 0, PORT_DEPTH * 2, TILE_SIZE))
    return west_band, east_band


def vertical_port_bands(tile: Image.Image) -> tuple[Image.Image, Image.Image]:
    """Return complementary north/south bands from a continuous source strip."""
    seam = strongest_horizontal_seam(tile)
    strip = tile.crop((0, seam - PORT_DEPTH, TILE_SIZE, seam + PORT_DEPTH))
    south_band = strip.crop((0, 0, TILE_SIZE, PORT_DEPTH))
    north_band = strip.crop((0, PORT_DEPTH, TILE_SIZE, PORT_DEPTH * 2))
    return north_band, south_band


def clear_band(tile: Image.Image, box: tuple[int, int, int, int]) -> None:
    tile.paste((0, 0, 0, 0), box)


def normalize_atlas(path: Path) -> None:
    with Image.open(path) as source:
        atlas = source.convert("RGBA")

    expected_size = TILE_SIZE * ATLAS_GRID_SIZE
    if atlas.size != (expected_size, expected_size):
        raise ValueError(f"{path}: expected {expected_size}x{expected_size}, got {atlas.size}")

    horizontal_straight = atlas.crop(tile_box(EAST | WEST))
    vertical_straight = atlas.crop(tile_box(NORTH | SOUTH))
    west_port, east_port = horizontal_port_bands(horizontal_straight)
    north_port, south_port = vertical_port_bands(vertical_straight)

    normalized = Image.new("RGBA", atlas.size)
    for mask in range(16):
        tile = atlas.crop(tile_box(mask))

        clear_band(tile, (0, 0, PORT_DEPTH, TILE_SIZE))
        clear_band(tile, (TILE_SIZE - PORT_DEPTH, 0, TILE_SIZE, TILE_SIZE))
        clear_band(tile, (0, 0, TILE_SIZE, PORT_DEPTH))
        clear_band(tile, (0, TILE_SIZE - PORT_DEPTH, TILE_SIZE, TILE_SIZE))

        if mask & WEST:
            tile.alpha_composite(west_port, (0, 0))
        if mask & EAST:
            tile.alpha_composite(east_port, (TILE_SIZE - PORT_DEPTH, 0))
        if mask & NORTH:
            tile.alpha_composite(north_port, (0, 0))
        if mask & SOUTH:
            tile.alpha_composite(south_port, (0, TILE_SIZE - PORT_DEPTH))

        normalized.alpha_composite(tile, tile_box(mask)[:2])

    validate_atlas(normalized, path)
    normalized.save(path, optimize=True)


def validate_atlas(atlas: Image.Image, path: Path) -> None:
    tiles = [atlas.crop(tile_box(mask)) for mask in range(16)]
    west_ports = {
        tiles[mask].crop((0, 0, PORT_DEPTH, TILE_SIZE)).tobytes()
        for mask in range(16)
        if mask & WEST
    }
    east_ports = {
        tiles[mask].crop((TILE_SIZE - PORT_DEPTH, 0, TILE_SIZE, TILE_SIZE)).tobytes()
        for mask in range(16)
        if mask & EAST
    }
    north_ports = {
        tiles[mask].crop((0, 0, TILE_SIZE, PORT_DEPTH)).tobytes()
        for mask in range(16)
        if mask & NORTH
    }
    south_ports = {
        tiles[mask].crop((0, TILE_SIZE - PORT_DEPTH, TILE_SIZE, TILE_SIZE)).tobytes()
        for mask in range(16)
        if mask & SOUTH
    }
    port_sets = (west_ports, east_ports, north_ports, south_ports)
    if any(len(ports) != 1 for ports in port_sets):
        raise ValueError(f"{path}: connected edge ports are not identical")


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("atlases", nargs="*", type=Path, default=DEFAULT_ATLASES)
    args = parser.parse_args()
    for atlas in args.atlases:
        normalize_atlas(atlas)
        print(f"normalized {atlas}")


if __name__ == "__main__":
    main()
