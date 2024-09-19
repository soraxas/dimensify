#!/bin/sh

pip install . && exec ipython -c '
import robotsim
import numpy as np
r = robotsim.Robot("../assets/panda/urdf/panda_relative.urdf")

print(r.joints)

joints = np.random.rand(55, 9)
joints = np.zeros([55, 9])
# r.has_collision(joints)

r.has_collision([[0]*9]*5)

limits = np.array(r.joint_limits_by_order)

def get_random_joints():
    return np.random.rand(len(limits)) * (limits[:, 1] - limits[:, 0]) + limits[:, 0]

' -i

