from tknetwork import Network, Peer
import tkinter as tk
from sys import argv
from json import dumps, loads


class Canvas:
    def __init__(self, network: Network) -> None:
        self.root = tk.Tk()
        self.canvas = tk.Canvas(self.root, height=500, width=500, bg="white")
        self.canvas.pack()
        self.network = network
        self.canvas.bind("<B1-Motion>", self.__draw)
        self.canvas.bind("<B3-Motion>", self.__erase)

    def draw(self, dict: dict):
        self.canvas.create_oval(
            dict["x"], dict["y"], dict["x"]+10, dict["y"]+10, fill="black")
        
    def erase(self, dict: dict):
        [*map(self.canvas.delete, self.canvas.find_overlapping(dict["x"]-10, dict["y"]-10, dict["x"]+10, dict["y"]+10))]


    def __draw(self, event):
        self.draw({"x": event.x, "y": event.y})
        self.network.emit("draw", dumps({"x": event.x, "y": event.y}))
        
    def __erase(self, event):
        self.erase({"x": event.x, "y": event.y})
        self.network.emit("erase", dumps({"x": event.x, "y": event.y}))
            
    def start(self):
        self.root.mainloop()


net = Network("0.0.0.0", 5000)


@net.events.bind("connect")
def connect(address):
    net.tcp_connect(address, 5000)


@net.events.bind("new_peer")
def connection(peer):
    print(f"Peer <{peer.address}> connected")

    @peer.events.bind("draw")
    def draw(data):
        canvas.draw(loads(data))
        
    @peer.events.bind("erase")
    def erase(data):
        canvas.erase(loads(data))


@net.events.bind("peer_disconnect")
def disconnect(peer):
    print(f"Peer <{peer.address}> disconnected")


if __name__ == "__main__":
    canvas = Canvas(net)

    if len(argv) == 1:
        net.udp_server()

    elif len(argv) >= 3 and argv[1] == "-c":
        net.tcp_server()
        net.connect(argv[2], 5000)

    canvas.start()
