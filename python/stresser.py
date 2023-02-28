from tknetwork import Network, Peer
from random import randint
from json import dumps, loads

net = Network("0.0.0.0", 5000)

net.serve(udp = False)

net.connect("127.0.0.1")

while True:
    input()
    for i in range(500):
        x = randint(0, 700)
        y = randint(0, 700)
        net.emit("draw", dumps({"x": x, "y": y, "hold": False, "color": "#ff0000"}))
        
