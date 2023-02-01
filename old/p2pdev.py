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
        s.bind(("0.0.0.0", 7337))
        s.listen()
        while True:
            client, adress = s.accept()
            Connection(client, adress)

    @classmethod
    def __accept_client(cls):
        s = socket(AF_INET, SOCK_DGRAM)
        s.bind(("0.0.0.0", 7337))
        while True:
            data, (address, _) = s.recvfrom(1024)
            if data:
                request = json.dumps({
                    "type": "connect",
                    "address": address
                }).encode()
                cls.broadcast(request)

                client = socket()
                client.connect((address, 7337))
                Connection(client, address)

    @classmethod
    def broadcast(cls, data: bytes):
        for connection in cls.connections:
            connection.socket.send(data)

    def start(self):
        threading.Thread(target=self.__listen, daemon=True).start()

    def __listen(self):
        while True:
            try:
                data: bytes = self.socket.recv(1024)
            except:
                self.__disconnect()
                return

            if data:
                request = json.loads(data.decode())
                self.__event_handler(request)

    def __disconnect(self):
        self.connections.remove(self)
        self.socket.close()

    @classmethod
    def __event_handler(cls, request: dict):
        try:
            cls.events[request["type"]](request)
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
        s.connect((request["address"], 7337))
        Connection(s, request["address"])

    events["connect"] = add_client

    @classmethod
    def connect(cls, address: str):
        s = socket(AF_INET, SOCK_DGRAM)
        s.sendto(b"hello", (address, 7337))


@Connection.on("draw")
def draw(request: dict):
    print(request["data"])


if __name__ == "__main__":
    if len(argv) == 1:
        Connection.serve()
    elif len(argv) >= 3 and argv[1] == "-c":
        Connection.serve()
        Connection.connect(argv[2])

    while (i := input()) != "exit":
        Connection.broadcast(json.dumps({"type": "draw", "data": i}).encode())
