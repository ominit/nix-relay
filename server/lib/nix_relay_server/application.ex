defmodule NixRelayServer.Application do
  use Application

  @impl true
  def start(_type, _args) do
    children = [
      {Bandit, plug: NixRelayServer.Router, scheme: :http, port: 4000},
      NixRelayServer.BuildQueue
    ]

    opts = [strategy: :one_for_one, name: NixRelayServer.Supervisor]
    Supervisor.start_link(children, opts)
  end
end
