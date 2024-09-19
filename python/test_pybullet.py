import pybullet as p
import time


# p.GUI

p.connect(p.GUI)


p.resetSimulation()
# p.configureDebugVisualizer(p.COV_ENABLE_RENDERING, 0)
# p.setRealTimeSimulation(1)

robot_id = p.loadURDF(
    "../assets/panda/urdf/panda_relative.urdf",
    useFixedBase=True,
    flags=p.URDF_USE_SELF_COLLISION,
)


p.performCollisionDetection()




for contact in p.getContactPoints():
    print((contact))
    # print(p.getBodyInfo(contact[1]))
    # print(p.getBodyInfo(contact[2]))

    j1 = p.getJointInfo(contact[1], contact[3])
    print(j1[1], j1[12])
    j2 = p.getJointInfo(contact[2], contact[4])
    print(j2[1], j2[12])

    # print(p.getJointInfo(contact[1], contact[3])[1])
    # print(p.getJointInfo(contact[2], contact[4])[1])
    # print(p.getBodyInfo(contact[1])[1].decode("utf-8"))
    # print(p.getBodyInfo(contact[2])[1].decode("utf-8"))
    print()
# time.sleep(10)
