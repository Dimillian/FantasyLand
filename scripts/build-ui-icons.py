#!/usr/bin/env python3
"""Build the shared 24px UI sprite. Integer pixels, one outline, fixed palette.
Edit these silhouettes to extend the set; never substitute font glyphs for icons.
Equipment art is authored separately in dist/ui/equipment-icons.svg at 64px.
"""

from collections import defaultdict
from pathlib import Path

P = {
    "ink": "#101719",
    "shade": "#39484b",
    "steel": "#81979a",
    "light": "#d3d6b4",
    "gold": "#b18a49",
    "shine": "#ead08b",
    "wood": "#715039",
    "leather": "#aa7750",
    "green": "#719454",
    "mint": "#b9ce7e",
    "red": "#af5140",
    "fire": "#e59b45",
    "blue": "#6babbc",
    "ice": "#b7d7d0",
    "purple": "#9279ad",
    "pink": "#c0a0ca",
}


class Icon:
    def __init__(self):
        self.p = {}

    def rect(self, x, y, w, h, c):
        for j in range(y, y + h):
            for i in range(x, x + w):
                self.p[i, j] = c
        return self

    def line(self, x, y, a, b, c, w=1):
        n = max(abs(a - x), abs(b - y))
        for k in range(n + 1):
            self.rect(
                round(x + (a - x) * k / max(n, 1)),
                round(y + (b - y) * k / max(n, 1)),
                w,
                w,
                c,
            )
        return self

    def poly(self, points, c):
        for y in range(min(p[1] for p in points), max(p[1] for p in points) + 1):
            for x in range(min(p[0] for p in points), max(p[0] for p in points) + 1):
                inside = False
                for (a, b), (d, e) in zip(points, points[1:] + points[:1]):
                    if (b > y + 0.5) != (e > y + 0.5) and x + 0.5 < (d - a) * (
                        y + 0.5 - b
                    ) / (e - b) + a:
                        inside = not inside
                if inside:
                    self.p[x, y] = c
        return self

    def svg(self, name):
        pixels = {
            (x + dx, y + dy): "ink"
            for x, y in self.p
            for dx, dy in [(0, -1), (0, 1), (-1, 0), (1, 0)]
        }
        pixels.update(self.p)
        colors = defaultdict(list)
        for (x, y), c in sorted(pixels.items(), key=lambda a: (a[0][1], a[0][0])):
            assert 0 <= x < 24 and 0 <= y < 24, (name, x, y)
            colors[c].append(f"M{x} {y}h1v1h-1z")
        return (
            f'<symbol id="{name}" viewBox="0 0 24 24">'
            + "".join(
                f'<path fill="{P[c]}" d="{"".join(paths)}"/>'
                for c, paths in colors.items()
            )
            + "</symbol>"
        )


icons = {}


def icon(name):
    icons[name] = Icon()
    return icons[name]


# Weapons and combat. Light comes from the upper left throughout the set.
icon("axe").line(7, 20, 16, 3, "wood", 2).line(8, 20, 17, 3, "leather").poly(
    [(10, 3), (17, 3), (21, 7), (21, 12), (16, 11), (12, 7), (7, 6)], "steel"
).line(17, 4, 20, 7, "light").line(20, 8, 20, 11, "light").rect(12, 5, 3, 3, "gold")
icon("mace").line(6, 19, 15, 7, "wood", 3).line(7, 19, 15, 8, "gold").poly(
    [
        (13, 2),
        (18, 2),
        (18, 4),
        (21, 4),
        (21, 9),
        (18, 9),
        (18, 12),
        (13, 12),
        (13, 10),
        (10, 10),
        (10, 5),
        (13, 5),
    ],
    "steel",
).rect(13, 3, 2, 7, "light").rect(17, 5, 3, 4, "shade")
icon("spear").line(3, 21, 18, 6, "wood", 2).line(4, 21, 19, 6, "leather").poly(
    [(17, 2), (22, 1), (21, 7), (16, 11), (14, 8)], "steel"
).line(20, 3, 16, 7, "light").line(13, 9, 16, 12, "gold")
icon("bow").poly(
    [
        (5, 2),
        (11, 4),
        (15, 8),
        (16, 12),
        (14, 17),
        (9, 21),
        (5, 22),
        (7, 19),
        (11, 16),
        (13, 12),
        (11, 8),
        (7, 5),
    ],
    "wood",
).line(6, 3, 12, 7, "leather").line(6, 3, 6, 20, "light").line(
    3, 12, 21, 12, "gold"
).poly([(18, 9), (22, 12), (18, 15)], "steel").line(3, 10, 5, 12, "mint")
icon("fist").rect(6, 6, 12, 9, "leather").rect(5, 4, 3, 5, "light").rect(
    9, 3, 3, 6, "light"
).rect(13, 4, 3, 5, "light").rect(17, 6, 3, 5, "leather").rect(
    4, 11, 5, 6, "leather"
).rect(9, 15, 9, 3, "wood").rect(8, 18, 11, 3, "gold").rect(10, 19, 2, 2, "shine")
# Magic: each school has its own recognizable silhouette and accent.
icon("fire").poly(
    [
        (11, 2),
        (14, 6),
        (13, 9),
        (18, 5),
        (17, 11),
        (21, 15),
        (19, 20),
        (15, 22),
        (8, 21),
        (4, 17),
        (5, 12),
        (8, 8),
        (8, 13),
        (11, 10),
    ],
    "red",
).poly(
    [(11, 7), (13, 12), (16, 11), (18, 16), (15, 20), (9, 19), (7, 16), (10, 12)],
    "fire",
).poly([(12, 13), (15, 17), (13, 20), (10, 18)], "shine")
icon("frost").line(11, 2, 11, 21, "ice", 2).line(3, 6, 19, 18, "blue", 2).line(
    3, 18, 19, 6, "blue", 2
).line(8, 3, 11, 6, "ice").line(15, 3, 12, 6, "ice").line(3, 10, 7, 9, "ice").line(
    3, 14, 7, 15, "ice"
).line(16, 9, 20, 10, "ice").line(16, 15, 20, 14, "ice").line(
    8, 20, 11, 17, "ice"
).line(15, 20, 12, 17, "ice").rect(10, 10, 4, 4, "light")
icon("storm").poly(
    [(12, 2), (20, 2), (14, 10), (19, 10), (6, 22), (10, 13), (5, 13)], "gold"
).poly(
    [(13, 3), (17, 3), (11, 11), (15, 11), (9, 17), (11, 12), (8, 12)], "shine"
).rect(3, 5, 2, 2, "blue").rect(19, 17, 2, 2, "blue")
icon("restoration").poly(
    [(7, 3), (17, 3), (21, 7), (21, 15), (17, 20), (7, 20), (3, 15), (3, 7)], "green"
).rect(9, 4, 5, 15, "mint").rect(5, 9, 14, 5, "mint").rect(10, 6, 3, 12, "shine").rect(
    7, 10, 10, 3, "shine"
)
icon("illusion").poly(
    [(2, 12), (6, 7), (11, 5), (16, 6), (22, 12), (17, 17), (12, 19), (6, 17)], "purple"
).poly([(4, 12), (8, 9), (15, 9), (20, 12), (16, 15), (8, 15)], "light").rect(
    10, 9, 5, 6, "purple"
).rect(12, 10, 2, 4, "ink").rect(10, 9, 2, 2, "ice")
icon("conjuration").poly(
    [(7, 3), (17, 3), (21, 8), (21, 17), (16, 21), (7, 21), (3, 16), (3, 8)], "purple"
).poly(
    [(8, 5), (16, 5), (18, 9), (18, 16), (15, 18), (8, 18), (6, 15), (6, 9)], "shade"
).poly([(11, 7), (16, 11), (13, 17), (8, 13)], "pink").rect(11, 10, 3, 3, "shine").rect(
    2, 2, 2, 2, "gold"
).rect(20, 20, 2, 2, "gold")
icon("alteration").poly(
    [(12, 2), (21, 8), (19, 18), (12, 22), (5, 18), (3, 8)], "gold"
).poly([(12, 5), (18, 9), (16, 16), (12, 19), (8, 16), (6, 9)], "green").line(
    8, 9, 12, 6, "mint"
).line(12, 6, 12, 16, "light").line(12, 16, 16, 10, "mint")
icon("necromancy").poly(
    [
        (7, 4),
        (17, 4),
        (20, 8),
        (20, 15),
        (16, 17),
        (16, 21),
        (8, 21),
        (8, 17),
        (4, 15),
        (4, 8),
    ],
    "steel",
).poly([(7, 6), (17, 6), (18, 10), (16, 14), (8, 14), (6, 10)], "light").rect(
    7, 10, 4, 4, "ink"
).rect(14, 10, 4, 4, "ink").rect(8, 11, 2, 2, "mint").rect(15, 11, 2, 2, "mint").rect(
    11, 15, 3, 2, "shade"
).rect(10, 18, 1, 3, "ink").rect(13, 18, 1, 3, "ink")
# Life skills: physical tools, not abstract runes.
icon("foraging").line(11, 8, 11, 21, "wood", 2).poly(
    [(11, 11), (5, 10), (2, 5), (7, 5), (11, 8)], "green"
).poly([(12, 8), (13, 3), (19, 2), (18, 7)], "mint").poly(
    [(12, 16), (16, 11), (22, 11), (19, 16)], "green"
).rect(5, 14, 3, 3, "red").rect(7, 17, 3, 3, "fire")
icon("woodcutting").rect(3, 15, 15, 6, "wood").rect(4, 14, 13, 2, "leather").rect(
    6, 17, 2, 3, "gold"
).rect(14, 16, 2, 4, "leather").line(10, 15, 17, 3, "wood", 2).poly(
    [(10, 3), (16, 3), (20, 6), (20, 10), (15, 9), (12, 6), (8, 6)], "steel"
).line(16, 4, 19, 7, "light")
icon("woodworking").rect(3, 17, 18, 4, "wood").rect(3, 17, 18, 1, "leather").poly(
    [(3, 10), (15, 4), (19, 8), (7, 16)], "steel"
).line(4, 10, 15, 5, "light").line(7, 14, 17, 8, "shade").poly(
    [(15, 4), (17, 2), (21, 4), (22, 7), (19, 10), (17, 8)], "leather"
).rect(18, 5, 2, 2, "ink").rect(5, 19, 3, 1, "gold")
icon("smithing").poly(
    [
        (2, 9),
        (21, 9),
        (17, 13),
        (14, 13),
        (14, 17),
        (18, 19),
        (18, 21),
        (5, 21),
        (5, 19),
        (9, 17),
        (9, 13),
        (5, 13),
    ],
    "steel",
).rect(3, 9, 17, 2, "light").rect(10, 13, 4, 4, "shade").line(
    9, 7, 14, 2, "wood", 2
).rect(13, 2, 7, 4, "gold").rect(13, 2, 7, 1, "shine")
icon("alchemy").rect(10, 2, 5, 2, "gold").rect(11, 4, 3, 5, "ice").poly(
    [(10, 8), (15, 8), (20, 16), (19, 21), (5, 21), (4, 16)], "blue"
).poly([(8, 13), (17, 13), (18, 17), (17, 19), (7, 19), (6, 17)], "purple").rect(
    7, 14, 2, 3, "pink"
).rect(10, 10, 2, 3, "light").rect(15, 17, 2, 2, "pink")
icon("cooking").rect(6, 9, 13, 10, "shade").rect(4, 10, 3, 5, "gold").rect(
    18, 10, 3, 5, "gold"
).rect(7, 8, 11, 2, "steel").rect(8, 11, 2, 6, "steel").rect(9, 20, 3, 2, "fire").rect(
    14, 20, 3, 2, "red"
).line(10, 6, 11, 3, "light").line(15, 6, 16, 2, "steel").rect(11, 14, 5, 2, "wood")
icon("fishing").line(5, 21, 14, 3, "wood", 2).line(15, 4, 20, 6, "light").line(
    20, 6, 20, 15, "light"
).poly(
    [(12, 14), (18, 13), (21, 16), (18, 19), (12, 18), (9, 20), (9, 13)], "blue"
).rect(13, 14, 4, 2, "ice").rect(18, 15, 1, 1, "ink")
icon("tailoring").poly([(3, 4), (13, 3), (20, 8), (17, 21), (6, 20)], "purple").rect(
    5, 5, 4, 13, "pink"
).line(8, 20, 17, 6, "light").rect(15, 5, 2, 2, "ink").line(17, 7, 20, 3, "gold").line(
    20, 3, 21, 12, "gold"
)
icon("leatherworking").poly(
    [
        (7, 2),
        (10, 5),
        (14, 5),
        (17, 2),
        (20, 7),
        (17, 10),
        (18, 17),
        (20, 20),
        (15, 22),
        (12, 19),
        (9, 22),
        (4, 20),
        (6, 16),
        (6, 10),
        (3, 7),
    ],
    "wood",
).poly([(9, 6), (14, 7), (16, 11), (14, 18), (8, 17), (7, 11)], "leather").line(
    9, 8, 13, 8, "gold"
).line(10, 10, 15, 17, "steel").rect(14, 16, 3, 4, "gold")
# Menus and small controls share the same materials and grid.
icon("bag").rect(7, 3, 10, 4, "wood").rect(9, 3, 6, 1, "leather").rect(
    9, 5, 6, 2, "ink"
).poly([(5, 7), (19, 7), (21, 20), (3, 20)], "leather").rect(5, 9, 14, 4, "wood").rect(
    10, 10, 4, 5, "gold"
).rect(11, 11, 2, 2, "shine").rect(6, 15, 3, 4, "gold")
icon("character").poly(
    [
        (8, 2),
        (16, 2),
        (19, 6),
        (18, 13),
        (21, 17),
        (21, 21),
        (3, 21),
        (3, 17),
        (6, 13),
        (5, 6),
    ],
    "green",
).poly([(8, 5), (16, 5), (16, 12), (12, 16), (8, 12)], "shade").rect(
    9, 6, 3, 5, "leather"
).rect(12, 16, 2, 5, "gold")
icon("book").poly(
    [
        (3, 4),
        (11, 5),
        (12, 7),
        (13, 5),
        (21, 4),
        (21, 20),
        (13, 21),
        (12, 20),
        (11, 21),
        (3, 20),
    ],
    "wood",
).rect(4, 5, 7, 14, "light").rect(13, 5, 7, 14, "gold").rect(
    12, 7, 1, 14, "leather"
).rect(5, 7, 5, 1, "wood").rect(5, 10, 4, 1, "wood").rect(14, 7, 5, 1, "shine").rect(
    14, 10, 4, 1, "shine"
)
icon("map").poly(
    [(3, 4), (8, 2), (15, 5), (21, 3), (21, 20), (15, 22), (8, 19), (3, 21)], "gold"
).poly([(4, 5), (8, 4), (8, 17), (4, 19)], "light").poly(
    [(9, 4), (14, 6), (14, 20), (9, 18)], "leather"
).poly([(15, 6), (20, 5), (20, 18), (15, 20)], "light").line(5, 13, 16, 9, "wood").line(
    16, 9, 18, 13, "wood"
).rect(16, 12, 3, 3, "red")
icon("menu").rect(4, 3, 16, 18, "wood").rect(5, 4, 14, 16, "shade").rect(
    8, 7, 9, 2, "gold"
).rect(8, 11, 9, 2, "gold").rect(8, 15, 9, 2, "gold").rect(6, 6, 1, 12, "steel")
icon("settings").poly(
    [
        (9, 2),
        (15, 2),
        (15, 5),
        (19, 5),
        (19, 9),
        (22, 9),
        (22, 15),
        (19, 15),
        (19, 19),
        (15, 19),
        (15, 22),
        (9, 22),
        (9, 19),
        (5, 19),
        (5, 15),
        (2, 15),
        (2, 9),
        (5, 9),
        (5, 5),
        (9, 5),
    ],
    "steel",
).rect(8, 8, 8, 8, "gold").rect(10, 10, 4, 4, "ink").rect(10, 3, 4, 2, "light")
icon("close").line(6, 6, 17, 17, "gold", 2).line(6, 17, 17, 6, "gold", 2).line(
    6, 6, 17, 17, "shine"
)
icon("plus").rect(10, 4, 4, 16, "gold").rect(4, 10, 16, 4, "gold").rect(
    10, 4, 1, 16, "shine"
).rect(4, 10, 16, 1, "shine")
icon("minus").rect(4, 10, 16, 4, "gold").rect(4, 10, 16, 1, "shine")
icon("arrow").rect(3, 10, 12, 3, "gold").poly(
    [(13, 5), (21, 11), (13, 18)], "gold"
).line(14, 6, 19, 10, "shine").rect(3, 10, 10, 1, "shine")
icon("center").rect(10, 2, 3, 20, "gold").rect(2, 10, 20, 3, "gold").rect(
    6, 6, 11, 11, "steel"
).rect(8, 8, 7, 7, "ink").rect(10, 10, 3, 3, "shine")
icon("expand").rect(3, 3, 6, 2, "gold").rect(3, 3, 2, 6, "gold").rect(
    15, 3, 6, 2, "gold"
).rect(19, 3, 2, 6, "gold").rect(3, 19, 6, 2, "gold").rect(3, 15, 2, 6, "gold").rect(
    15, 19, 6, 2, "gold"
).rect(19, 15, 2, 6, "gold")
icon("coin").poly(
    [(8, 3), (16, 3), (20, 7), (20, 17), (16, 21), (8, 21), (4, 17), (4, 7)], "gold"
).line(8, 4, 15, 4, "shine").line(5, 8, 5, 16, "shine").rect(
    10, 7, 3, 11, "leather"
).rect(8, 9, 7, 2, "shine")
icon("info").poly(
    [(7, 3), (17, 3), (21, 7), (21, 17), (17, 21), (7, 21), (3, 17), (3, 7)], "shade"
).rect(10, 5, 3, 3, "shine").rect(10, 10, 3, 8, "gold").rect(8, 18, 7, 2, "gold")
root = Path(__file__).resolve().parent.parent
(root / "dist/ui/icons.svg").write_text(
    '<svg xmlns="http://www.w3.org/2000/svg" shape-rendering="crispEdges">'
    + "".join(i.svg(n) for n, i in icons.items())
    + "</svg>\n"
)
print(f"Built {len(icons)} pixel icons.")
