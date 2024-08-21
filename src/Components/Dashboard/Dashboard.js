import PropTypes from "prop-types";
import "./Dashboard.scss";
import Header from "../Header/Header";
import Library from "../Library/Library";

const Dashboard = ({ config }) => {
  return (
    <div id={"dashboard"}>
      <Header />
      Hello from configured
      <br />
      You config is {JSON.stringify(config)}
      <br />
      <Library />
    </div>
  );
};

Dashboard.propTypes = {
  config: PropTypes.object.isRequired,
};

export default Dashboard;
