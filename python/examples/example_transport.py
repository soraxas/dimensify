import json
import time

import dimensify
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

    # world = World(server_addr="127.0.0.1:6210", mode="udp")
    # world.spawn(
    #     Mesh3d(name="cube", position=(0,0,0), scale=(1,1,1)),
    #     Transform3d(position=(0,0,0), rotation=(0,0,0,1), scale=(1,1,1)),
    #     Line3d(points=[(0,0,0),(1,1,1)], color=(1,1,1,1)),
    # )
    # exit()

    client = TransportClient(server_addr="127.0.0.1:6210", mode="udp")

    for i in range(10):
        command = {
            "Spawn": {
                "components": [
                    {"type": "Name", "value": "demo_cube"},
                    {
                        "type": "Mesh3d",
                        "name": "demo_cube",
                        # "position": [0.0, 1.0, 0.0],
                        "position": rand_list(5.0),
                        # "scale": [1.0, 1.0, 1.0],
                        "scale": rand_list(3.0),
                    },
                    {
                        "type": "Line3d",
                        "points": [[0.0, 0.0, 0.0], [1.2, 0.6, 0.4]],
                        "color": [0.2, 0.8, 1.0, 1.0],
                        "width": 1.0,
                    },
                ]
            }
        }

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
                            "position": [0.4, 0.6, 0.2],
                            "rotation": [0.0, 0.0, 0.0, 1.0],
                            "scale": [1.2, 0.8, 1.0],
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
