import tkinter as tk
from socket import *
from json import loads, dumps
import threading
import re


class Canvas:
    def __init__(self, app) -> None:
        self.root = tk.Tk()
        self.canvas = tk.Canvas(self.root, height=500, width=500, bg="white")
        self.canvas.pack()
        self.app = app
        self.canvas.bind("<B1-Motion>", self._drag)
    
    def draw(self, dict: dict):
        self.canvas.create_oval(dict["x"], dict["y"], dict["x"]+10, dict["y"]+10, fill="black")

    def _drag(self, event):
        self.draw({"x": event.x, "y": event.y})
        self.app.send({"x": event.x, "y": event.y})

    def start(self):
        self.root.mainloop()


class App:
    def __init__(self) -> None:
        pass

    def connect(self, ip: str, port: int):
        s = socket()
        s.connect((ip, port))
        self.socket = s
        self.start()

    def serve(self):
        threading.Thread(target=self.__serve).start()

    def __serve(self):
        s = socket(AF_INET, SOCK_STREAM)
        host = "0.0.0.0"
        port = 12345
        s.bind((host, port))
        s.listen()
        self.socket, _ = s.accept()
        self.start()

    def start(self):
        self.canvas = Canvas(self)
        threading.Thread(target=self.__listen).start()
        self.canvas.start()

    def send(self, data: dict):
        self.socket.send(dumps(data).encode())

    def __listen(self):
        while True:
            data = self.socket.recv(1024)
            if data:
                try:
                    data = data.decode()
                    for i in re.split(r"\{.*?\}", data):
                        if i:
                            self.canvas.draw(loads(i))                    
                except:
                    print("Error while decoding data")
                    print(data)
                


if __name__ == "__main__":
    app = App()
    ip = input("Enter IP of the server or press enter to host: ")
    if ip:
        app.connect(ip, 12345)
    else:
        app.serve()
