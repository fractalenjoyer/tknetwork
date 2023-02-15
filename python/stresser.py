from tknetwork import Network, Peer
from json import dumps, loads

net = Network("0.0.0.0", 5000)

net.serve((False, True))

net.connect("127.0.0.1", 5000)

while True:
    net.emit("draw", dumps({"x": 0, "y": 0, "hold": False}))
