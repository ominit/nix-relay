defmodule NixRelayServer.WorkerWebSocketHandler do
  def init(_params) do
    {:ok, %{}}
  end

  def handle_in({"register", [opcode: :text]}, state) do
    IO.puts("Received message: register")
    NixRelayServer.BuildQueue.add_worker(self())
    {:ok, state}
  end

  def handle_in({"complete " <> rest, [opcode: :text]}, state) do
    [success, derivation] = String.split(rest, " ")
    IO.puts("complete #{derivation} #{success}")
    NixRelayServer.BuildQueue.complete(self(), success)
    {:ok, state}
  end

  def handle_info({:request_build, {_, derivation, data}}, _state) do
    state = derivation
    {:push, {:text, "request-build #{derivation} #{data}"}, state}
  end

  # Invoked when the connection is closed
  def terminate(_reason, _state) do
    IO.puts("Worker WebSocket connection closed")
    NixRelayServer.BuildQueue.remove_worker(self())
    :ok
  end
end
