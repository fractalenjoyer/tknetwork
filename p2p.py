from socket import *
import threading
import json
from sys import argv


class Connection:
    connections = []
    events = {}

    def __init__(self, socket, adress) -> None:
        self.socket = socket
        self.adress = adress
        self.connections.append(self)
        self.start()

    @classmethod
    def serve(cls, server: bool = True):
        if server:
            threading.Thread(target=cls.__serve, daemon=True).start()
        threading.Thread(target=cls.__accept_client, daemon=True).start()

    @classmethod
    def __serve(cls):
        s = socket(AF_INET, SOCK_STREAM)
        s.bind(("0.0.0.0", 12345))
        s.listen()
        while True:
            client, adress = s.accept()
            Connection(client, adress)

    @classmethod
    def __accept_client(cls):
        s = socket(AF_INET, SOCK_DGRAM)
        s.bind(("0.0.0.0", 6969))
        while True:
            data, (address, _) = s.recvfrom(1024)
            if data:
                request = json.dumps({
                    "type": "connect",
                    "address": address
                }).encode()
                cls.broadcast(request)

                client = socket()
                client.connect((address, 12345))
                Connection(client, address)

    @classmethod
    def broadcast(cls, data: bytes):
        for connection in cls.connections:
            connection.socket.send(data)

    def start(self):
        threading.Thread(target=self.__listen, daemon=True).start()

    def __listen(self):
        while True:
            data: bytes = self.socket.recv(1024)
            if data:
                request = json.loads(data.decode())
                self.event_handler(request)

    def event_handler(self, request: dict):
        try:
            self.events[request["type"]](self, request)
        finally:
            pass

    @classmethod
    def on(cls, event: str):

        def decorator(func):

            def wrapper(request: dict):
                func(request)

            cls.events[event] = func
            return wrapper

        return decorator

    @staticmethod
    def add_client(request: dict):
        s = socket()
        s.connect((request["address"], 12345))
        Connection(s, request["address"])
    
    events["connect"] = add_client

    @classmethod
    def connect(cls, address: str):
        s = socket()
        s.sendto(b"hello", (address, 6969))



@Connection.on("draw")
def draw(request: dict):
    print(request["data"])


if __name__ == "__main__":
    Connection.serve()
    if len(argv) >= 3 and argv[1] == "-c":
        Connection.connect(argv[2], 12345)

    while (i := input()) != "exit":
        Connection.broadcast(json.dumps({"type": "draw", "data": i}).encode())
