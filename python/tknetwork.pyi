class Event:
    def __call__(func: function): ...

class Peer:
    name: str
    def on(self, event: str) -> function: ...
    def emit(self, event: str, data: str): ...

class Network:
    def __init__(ip: str, port: int): ...
    def connect(self, ip: str, port: int): ...
    def on(self, event: str) -> function: ...
    def emit(self, event: str, data: str): ...
    """
    Start the server.
    debug: (disable_tcp, disable_udp)
    """
    def serve(self, debug: tuple[bool, bool] = (False, False)): ...
    
    