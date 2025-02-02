defmodule NixRelayServer.BuildQueue do
  use GenServer

  def start_link(_opts), do: GenServer.start_link(__MODULE__, :ok, name: __MODULE__)
  def add(derivation), do: GenServer.cast(__MODULE__, {:add, derivation})
  def list, do: GenServer.call(__MODULE__, :list)

  def init(:ok), do: {:ok, []}

  def handle_cast({:add, derivation}, state) do
    IO.puts("Added to queue: #{derivation}")
    {:noreply, [derivation | state]}
  end

  def handle_call(:list, _from, state) do
    {:reply, state, state}
  end
end
