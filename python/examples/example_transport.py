import json
import time

import dimensify
import dimensify as d
from dimensify import TransportClient
import random

from pprint import pprint


def rand_list(f: float, n: int = 3):
    out = []
    for i in range(n):
        out.append(f * random.random())
    return out


def main() -> None:
    print("module:", dimensify.__file__)

    world = d.World(server_addr="127.0.0.1:6210", mode="udp")
    client = TransportClient(server_addr="127.0.0.1:6210", mode="udp")

    # all_entities = client.list(timeout_ms=10000)
    all_entities = []

    if len(all_entities) < 50:
        for i in range(50):
            translation = rand_list(15.0)

            r = random.random()
            rgba = [random.random() for _ in range(4)]
            entity = world.spawn(
                d.Component.name(f"sphere_{i}"),
                d.Component.transform(translation=translation),
                d.Component.mesh_3d(shape=d.Shape3d.sphere(radius=r)),
                d.Component.material_from_color(
                    r=rgba[0], g=rgba[1], b=rgba[2], a=rgba[3]
                ),
                # wait=False,
            )
            print("spawn response:", entity)
            time.sleep(0.001)
            if entity is not None:
                world.despawn(entity, timeout_ms=5000)

    for entity in all_entities:
        pprint(entity.to_dict())

    exit()

    for i in range(10):
        client.apply(json.dumps(command), timeout_ms=5000)
        print("--------------------------------")
        print("entities:")
        for entity in client.list(timeout_ms=5000):
            pprint(entity.to_dict())

        time.sleep(0.5)
        client.apply(
            json.dumps(
                {
                    "Update": {
                        "entity": "demo_cube",
                        "component": {
                            "type": "Transform3d",
                            "transform": {
                                "position": [0.4, 0.6, 0.2],
                                "rotation": [0.0, 0.0, 0.0, 1.0],
                                "scale": [1.2, 0.8, 1.0],
                            },
                        },
                    }
                }
            ),
            timeout_ms=5000,
        )

        time.sleep(0.5)
        client.remove("demo_cube", timeout_ms=5000)
        print("after remove:", client.list(timeout_ms=5000))


if __name__ == "__main__":
    main()
