// import { JetRuntime } from "jet:runtime";

export async function handle(request, context) {
  const { JetRuntime } = await import("jet:runtime");

  console.log("request", request);
  console.log("context", context);

  return new JetRuntime.Response(
    200,
    `Hello ${request.to}, this is ${context.current_user.name}.`,
  );
}
