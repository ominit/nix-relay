defmodule NixRelayServer.BuildQueue do
  use GenServer

  def start_link(_), do: GenServer.start_link(__MODULE__, :ok, name: __MODULE__)

  def add_job(pid, derivation, data),
    do: GenServer.cast(__MODULE__, {:add_job, {pid, derivation, data}})

  def add_worker(pid), do: GenServer.cast(__MODULE__, {:add_worker, pid})

  def remove_worker(pid), do: GenServer.cast(__MODULE__, {:remove_worker, pid})

  def complete(worker_pid, success),
    do: GenServer.cast(__MODULE__, {:complete, worker_pid, success})

  def init(:ok), do: {:ok, {:queue.new(), Map.new(), :queue.new()}}

  def handle_cast({:add_job, job}, {queue, pending, workers}) do
    queue = :queue.in(job, queue)
    {queue, pending, workers} = give_job({queue, pending, workers})
    {:noreply, {queue, pending, workers}}
  end

  def handle_cast({:add_worker, pid}, {queue, pending, workers}) do
    workers = :queue.in(pid, workers)
    {queue, pending, workers} = give_job({queue, pending, workers})
    {:noreply, {queue, pending, workers}}
  end

  def handle_cast({:remove_worker, worker_pid}, {queue, pending, workers}) do
    case(Map.pop(pending, worker_pid)) do
      {nil, ^pending} ->
        new_workers = :queue.filter(fn pid -> pid != worker_pid end, workers)
        {:noreply, {queue, pending, new_workers}}

      {job, new_pending} ->
        new_queue = :queue.in(job, queue)
        {:noreply, {new_queue, new_pending, workers}}
    end
  end

  def handle_cast({:complete, worker_pid, success}, {queue, pending, workers}) do
    case Map.pop(pending, worker_pid) do
      {nil, pending} ->
        {:noreply, {queue, pending, workers}}

      {{client_pid, derivation, _}, pending} ->
        IO.puts("buildqueue send to client #{derivation}")
        send(client_pid, {:complete, derivation, success})
        new_workers = :queue.in(worker_pid, workers)
        {new_queue, new_pending, final_workers} = give_job({queue, pending, new_workers})
        {:noreply, {new_queue, new_pending, final_workers}}
    end
  end

  defp give_job({queue, pending, workers}) do
    if(!:queue.is_empty(queue) && !:queue.is_empty(workers)) do
      {{:value, job}, new_queue} = :queue.out(queue)
      {{:value, worker}, new_workers} = :queue.out(workers)
      send(worker, {:request_build, job})
      new_pending = Map.put(pending, worker, job)
      {new_queue, new_pending, new_workers}
    else
      {queue, pending, workers}
    end
  end
end
