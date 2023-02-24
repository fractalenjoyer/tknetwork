from tknetwork import Network, Peer
import tkinter as tk
from tkinter import colorchooser
import random
from sys import argv
from json import dumps, loads


class Canvas:
    def __init__(self, network: Network) -> None:
        self.prev_point = {}
        self.color = f"#{random.randint(0, 255):02x}{random.randint(0, 255):02x}{random.randint(0, 255):02x}"

        self.root = tk.Tk()
        self.root.resizable(False, False)
        button = tk.Button(self.root, text="Choose Color",
                           command=self.choose_color)
        button.pack()

        self.canvas = tk.Canvas(self.root, height=700, width=700, bg="white")
        self.canvas.pack()
        self.network = network

        self.canvas.bind("<B1-Motion>", lambda event: self.__draw(event, True))
        self.canvas.bind("<1>", lambda event: self.__draw(event, False))
        self.canvas.bind("<B3-Motion>", self.__erase)

    def draw(self, x, y, hold, color, peer):
        if hold and peer in self.prev_point.keys():
            pre_x, pre_y = self.prev_point[peer]
            self.canvas.create_line(
                pre_x, pre_y, x, y, width=10, fill=color, capstyle=tk.ROUND)
            self.prev_point[peer] = (x, y)
        elif not hold:
            self.prev_point[peer] = (x, y)
            self.canvas.create_oval(
                x-5, y-5, x+5, y+5, fill=color, outline=color)

    def erase(self, x, y):
        for item in self.canvas.find_overlapping(x-10, y-10, x+10, y+10):
            self.canvas.delete(item)

    def __draw(self, event, hold):
        self.draw(event.x, event.y, hold, self.color, self.network)
        self.network.emit("draw", dumps(
            {"x": event.x, "y": event.y, "hold": hold, "color": self.color}))

    def __erase(self, event):
        self.erase(event.x, event.y)
        self.network.emit("erase", dumps({"x": event.x, "y": event.y}))

    def choose_color(self):
        self.color = colorchooser.askcolor()[1]

    def start(self):
        self.root.mainloop()


net = Network("0.0.0.0", 5000)


@net.on("connect")
def connection(peer: Peer):
    print(f"Peer <{peer.name}> connected")

    @peer.on("draw")
    def draw(data):
        x, y, hold, color = loads(data).values()
        canvas.draw(x, y, hold, color, peer)

    @peer.on("erase")
    def erase(data):
        canvas.erase(**loads(data))


@net.on("disconnect")
def disconnect(peer):
    print(f"Peer <{peer.name}> disconnected")


if __name__ == "__main__":
    canvas = Canvas(net)

    if len(argv) == 1:
        net.serve(tcp=False)
    else:
        net.serve(udp=False)

    if len(argv) >= 3 and argv[1] == "-c":
        net.connect(argv[2], 5000)

    canvas.start()
