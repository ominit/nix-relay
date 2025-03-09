defmodule NixRelayServer.BuildQueueTest do
  use ExUnit.Case, async: false

  setup do
    # Start fresh for each test
    start_supervised!(NixRelayServer.BuildQueue)
    :ok
  end

  describe "job assignment" do
    test "jobs are assigned to available workers" do
      # Add a job
      client1 = self()
      derivation = "test-derivation"
      data = "test-data"

      # Create a worker process that will stay alive
      worker_pid = create_persistent_worker()

      # Register our job and worker with the build queue
      NixRelayServer.BuildQueue.add_job(client1, derivation, data)
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # We should receive the job request
      assert_receive {:request_build, {^client1, ^derivation, ^data}}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "multiple jobs are assigned in order" do
      # Add multiple jobs
      client_pid = self()
      derivation1 = "test-derivation-1"
      derivation2 = "test-derivation-2"
      data1 = "test-data-1"
      data2 = "test-data-2"

      NixRelayServer.BuildQueue.add_job(client_pid, derivation1, data1)
      NixRelayServer.BuildQueue.add_job(client_pid, derivation2, data2)

      # Create a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # First job should be assigned first
      assert_receive {:request_build, {^client_pid, ^derivation1, ^data1}}

      # Complete the first job
      NixRelayServer.BuildQueue.complete(worker_pid, "true")

      # Receive completion message
      assert_receive {:complete, ^derivation1, "true"}

      # Second job should be assigned next
      assert_receive {:request_build, {^client_pid, ^derivation2, ^data2}}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "jobs wait in queue when no workers are available" do
      # Add a job
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"

      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # No worker yet, so job should wait in queue
      refute_receive {:request_build, _}, 100

      # Now add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # Job should now be assigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Cleanup
      kill_worker(worker_pid)
    end
  end

  describe "worker management" do
    test "workers wait in queue when no jobs are available" do
      # Add a worker first
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # No jobs yet, so worker should be idle
      refute_receive {:request_build, _}, 100

      # Add a job
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"
      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # Worker should now get the job
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "multiple workers get assigned jobs as they arrive" do
      # Add multiple workers
      worker_pid1 = create_persistent_worker()
      worker_pid2 = create_persistent_worker()

      NixRelayServer.BuildQueue.add_worker(worker_pid1)
      NixRelayServer.BuildQueue.add_worker(worker_pid2)

      # Add jobs
      client_pid = self()
      derivation1 = "test-derivation-1"
      derivation2 = "test-derivation-2"
      data1 = "test-data-1"
      data2 = "test-data-2"

      NixRelayServer.BuildQueue.add_job(client_pid, derivation1, data1)
      NixRelayServer.BuildQueue.add_job(client_pid, derivation2, data2)

      # Should receive both jobs, one for each worker
      # (Note: we can't guarantee which worker gets which job)
      assert_receive {:request_build, {^client_pid, derivation_a, data_a}}
      assert_receive {:request_build, {^client_pid, derivation_b, data_b}}

      # Check that we received both unique jobs
      assert {derivation_a, data_a} != {derivation_b, data_b}
      assert derivation_a in [derivation1, derivation2]
      assert derivation_b in [derivation1, derivation2]
      assert data_a in [data1, data2]
      assert data_b in [data1, data2]

      # Cleanup
      kill_worker(worker_pid1)
      kill_worker(worker_pid2)
    end
  end

  describe "worker removal" do
    test "removing idle worker" do
      # Add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # Remove the worker
      NixRelayServer.BuildQueue.remove_worker(worker_pid)

      # Add a job - it should stay in the queue
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"
      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # Job should not be assigned since we removed the worker
      refute_receive {:request_build, _}, 100

      # Add another worker - it should get the job
      new_worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(new_worker_pid)

      # Now job should be assigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Cleanup
      kill_worker(worker_pid)
      kill_worker(new_worker_pid)
    end

    test "removing busy worker puts job back in queue" do
      # Add a job
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"
      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # Add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # Job should be assigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Remove the worker while it's busy
      NixRelayServer.BuildQueue.remove_worker(worker_pid)

      # Add another worker - it should get the same job back
      new_worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(new_worker_pid)

      # Same job should be reassigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Cleanup
      kill_worker(worker_pid)
      kill_worker(new_worker_pid)
    end
  end

  describe "job completion" do
    test "completing job notifies client of success" do
      # Add a job
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"
      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # Add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # Job should be assigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Complete the job with success
      NixRelayServer.BuildQueue.complete(worker_pid, "true")

      # Client should be notified
      assert_receive {:complete, ^derivation, "true"}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "completing job notifies client of failure" do
      # Add a job
      client_pid = self()
      derivation = "test-derivation"
      data = "test-data"
      NixRelayServer.BuildQueue.add_job(client_pid, derivation, data)

      # Add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # Job should be assigned
      assert_receive {:request_build, {^client_pid, ^derivation, ^data}}

      # Complete the job with failure
      NixRelayServer.BuildQueue.complete(worker_pid, "false")

      # Client should be notified
      assert_receive {:complete, ^derivation, "false"}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "worker is available for new jobs after completion" do
      # Add multiple jobs
      client_pid = self()
      derivation1 = "test-derivation-1"
      derivation2 = "test-derivation-2"
      data1 = "test-data-1"
      data2 = "test-data-2"

      NixRelayServer.BuildQueue.add_job(client_pid, derivation1, data1)
      NixRelayServer.BuildQueue.add_job(client_pid, derivation2, data2)

      # Add a worker
      worker_pid = create_persistent_worker()
      NixRelayServer.BuildQueue.add_worker(worker_pid)

      # First job should be assigned
      assert_receive {:request_build, {^client_pid, ^derivation1, ^data1}}

      # Complete the first job
      NixRelayServer.BuildQueue.complete(worker_pid, "true")

      # Client should be notified about first job
      assert_receive {:complete, ^derivation1, "true"}

      # Second job should now be assigned to the same worker
      assert_receive {:request_build, {^client_pid, ^derivation2, ^data2}}

      # Cleanup
      kill_worker(worker_pid)
    end

    test "completion from unknown worker is ignored" do
      # Try to complete a job from an unknown worker
      unknown_pid = spawn(fn -> nil end)
      NixRelayServer.BuildQueue.complete(unknown_pid, "true")

      # Nothing should happen (no crash)
      refute_receive _, 100
    end
  end

  # Helper functions to create and kill worker processes
  defp create_persistent_worker do
    parent = self()

    spawn_link(fn ->
      # This process will stay alive and forward messages to the parent
      receive do
        msg ->
          send(parent, msg)
          # Keep the process alive waiting for more messages
          create_persistent_worker_loop(parent)
      end
    end)
  end

  defp create_persistent_worker_loop(parent) do
    receive do
      msg ->
        send(parent, msg)
        create_persistent_worker_loop(parent)
    end
  end

  defp kill_worker(pid) do
    Process.exit(pid, :normal)
    # Give it a moment to clean up
    Process.sleep(10)
  end
end
